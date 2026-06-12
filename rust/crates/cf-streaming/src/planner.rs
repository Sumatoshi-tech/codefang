//! Chunk-boundary planning and budget-aware scheduling for streaming execution.
//!
//! Provides the static [`Planner`], the feedback-driven [`AdaptivePlanner`],
//! memory-pressure detection, and the unified budget [`compute_schedule`]
//! decomposition.
//!
//! All commit counts and byte quantities are `i64`, so the integer arithmetic
//! — including truncating division — is platform-independent and matches the
//! reference implementation exactly (chunk plans steer analyzer state
//! aggregation, which the differential gate pins indirectly).

use cf_alg::{chunk, Range};
use cf_alg_stats::{exceeds_threshold, Ema};
use cf_units::{KIB, MIB};

/// A chunk of commits to process: a half-open `[start, end)` interval.
///
/// Alias for [`cf_alg::Range`] so `alg` consumers and streaming consumers
/// exchange values without conversion. Its `start`/`end` fields are `usize`
/// (commit indices are never negative); the planner's byte/budget math is done
/// in `i64`, and the two are bridged only at the [`cf_alg::chunk`] boundary
/// via the private `chunk_i64` helper.
pub type ChunkBounds = Range;

/// Bridges the planner's `i64` commit count + chunk size into [`cf_alg::chunk`],
/// whose API is `usize`. Callers guarantee `total > 0` and `size > 0`, so the
/// conversions never wrap; a defensive `try_into` falls back to an empty plan
/// if a caller ever violates that (the same non-positive guard the planner
/// applies).
fn chunk_i64(total: i64, size: i64) -> Vec<ChunkBounds> {
    let (Ok(total), Ok(size)): (Result<usize, _>, Result<usize, _>) =
        (total.try_into(), size.try_into())
    else {
        return Vec::new();
    };

    chunk(total, size)
}

// ---------------------------------------------------------------------------
// Planner constraints
// ---------------------------------------------------------------------------

/// Minimum commits per chunk to amortize hibernation cost.
pub const MIN_CHUNK_SIZE: i64 = 50;

/// Safety cap on commits per chunk. The primary constraint is the memory budget
/// divided by per-analyzer growth rate, not this cap.
pub const MAX_CHUNK_SIZE: i64 = 3000;

/// Fixed memory overhead for the runtime + libgit2 + caches (400 MiB).
pub const BASE_OVERHEAD: i64 = 400 * MIB;

/// Safety margin added to the aggregate growth rate to account for transient
/// pipeline allocations (tree diffs, blobs in flight, GC headroom) that scale
/// with chunk size but aren't captured by analyzer-declared growth.
const SAFETY_MARGIN_PERCENT: i64 = 50;

/// Converts [`SAFETY_MARGIN_PERCENT`] to a fraction.
const PERCENT_DIVISOR: i64 = 100;

/// Conservative fallback per-commit state growth when
/// [`Planner::aggregate_growth_per_commit`] is zero (e.g. in tests or when no
/// analyzers are selected). Equals [`DEFAULT_WORKING_STATE_SIZE`] +
/// [`DEFAULT_AVG_TC_SIZE`] (500 KiB).
pub const DEFAULT_STATE_GROWTH_PER_COMMIT: i64 = 500 * KIB;

/// Fallback per-commit working state estimate (400 KiB).
pub const DEFAULT_WORKING_STATE_SIZE: i64 = 400 * KIB;

/// Fallback per-commit TC payload estimate (100 KiB).
pub const DEFAULT_AVG_TC_SIZE: i64 = 100 * KIB;

/// Calculates chunk boundaries for streaming execution.
///
/// Construct with [`Planner::default`] and set the public fields, or use the
/// struct literal directly.
#[derive(Debug, Clone, Copy, Default)]
pub struct Planner {
    /// Number of commits to process.
    pub total_commits: i64,
    /// User-specified memory budget in bytes. Zero or negative means unlimited.
    pub memory_budget: i64,
    /// Summed per-commit state growth across all selected leaf analyzers. When
    /// zero, [`DEFAULT_STATE_GROWTH_PER_COMMIT`] is used.
    pub aggregate_growth_per_commit: i64,
    /// Estimated memory consumed by caches, workers, and buffers (everything
    /// except analyzer state). When positive, it replaces [`BASE_OVERHEAD`].
    pub pipeline_overhead: i64,
}

impl Planner {
    /// Returns chunk boundaries as `[start, end)` index pairs.
    ///
    /// Returns an empty vector when `total_commits <= 0`.
    #[must_use]
    pub fn plan(&self) -> Vec<ChunkBounds> {
        if self.total_commits <= 0 {
            return Vec::new();
        }

        chunk_i64(self.total_commits, self.calculate_chunk_size())
    }

    /// Determines the optimal chunk size based on budget and the aggregate
    /// per-commit growth rate of selected analyzers.
    fn calculate_chunk_size(&self) -> i64 {
        if self.memory_budget <= 0 {
            return MAX_CHUNK_SIZE;
        }

        // Available memory for analyzer state (after overhead).
        let overhead = if self.pipeline_overhead > 0 {
            self.pipeline_overhead
        } else {
            BASE_OVERHEAD
        };

        let available = self.memory_budget - overhead;
        if available <= 0 {
            return MIN_CHUNK_SIZE;
        }

        let mut growth = self.aggregate_growth_per_commit;
        if growth <= 0 {
            growth = DEFAULT_STATE_GROWTH_PER_COMMIT;
        }

        // Add safety margin for transient pipeline allocations.
        growth += growth * SAFETY_MARGIN_PERCENT / PERCENT_DIVISOR;

        // Max commits that fit in available memory.
        let max_commits = available / growth;

        // max(min(max_commits, MAX_CHUNK_SIZE), MIN_CHUNK_SIZE); MIN <= MAX
        // always (50 <= 3000), so this is exactly clamp(max_commits, MIN, MAX).
        max_commits.clamp(MIN_CHUNK_SIZE, MAX_CHUNK_SIZE)
    }

    /// Returns chunk boundaries for commits `[start_commit..total_commits)`.
    ///
    /// Used by the adaptive planner to re-plan remaining chunks after observing
    /// actual growth rates. Returns an empty vector when nothing remains.
    #[must_use]
    pub fn plan_from(&self, start_commit: i64) -> Vec<ChunkBounds> {
        let remaining = self.total_commits - start_commit;
        if remaining <= 0 {
            return Vec::new();
        }

        let sub = Self {
            total_commits: remaining,
            ..*self
        };

        // start_commit is non-negative here (remaining > 0 implies it is less
        // than total_commits >= 0); convert once to offset the usize indices.
        let offset = start_commit.max(0) as usize;
        let mut sub_chunks = sub.plan();
        for c in &mut sub_chunks {
            c.start += offset;
            c.end += offset;
        }

        sub_chunks
    }
}

// ---------------------------------------------------------------------------
// Adaptive planner
// ---------------------------------------------------------------------------

/// Triggers re-planning when observed growth diverges from predicted by more
/// than 25%.
pub const DEFAULT_REPLAN_THRESHOLD: f64 = 0.25;

/// EMA smoothing factor. 0.3 gives ~3-chunk half-life.
pub const DEFAULT_EMA_ALPHA: f64 = 0.3;

/// Floor for observed per-commit growth (1 KiB). Prevents zero/negative chunk
/// sizes when hibernation frees more than allocated.
const MIN_OBSERVED_GROWTH: i64 = KIB;

/// Telemetry from the adaptive planner.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AdaptiveStats {
    /// Number of re-plans performed.
    pub replan_count: i32,
    /// Final growth rate used (work EMA value if initialized, else declared).
    pub final_growth_rate: f64,
    /// Declared growth rate the planner was seeded with.
    pub initial_growth_rate: f64,
    /// Final working-state growth EMA.
    pub final_work_growth: f64,
    /// Final TC payload size EMA.
    pub final_tc_size: f64,
    /// Final aggregator state growth EMA.
    pub final_agg_growth: f64,
}

/// Per-chunk metric observations for adaptive replanning.
#[derive(Debug, Clone)]
pub struct ReplanObservation {
    /// Zero-based index of the chunk just processed.
    pub chunk_index: i32,
    /// Bounds of the chunk just processed.
    pub chunk: ChunkBounds,
    /// Observed per-commit working state growth in bytes (heap-in-use delta
    /// minus aggregator state delta, per commit).
    pub work_growth_per_commit: i64,
    /// Observed per-commit TC payload size in bytes.
    pub tc_payload_per_commit: i64,
    /// Observed per-commit aggregator state growth in bytes.
    pub agg_growth_per_commit: i64,
    /// Current chunk plan (including already-processed chunks).
    pub current_chunks: Vec<ChunkBounds>,
}

/// Wraps the static [`Planner`] with feedback-driven re-planning.
///
/// After each chunk it examines three separate metrics (working state growth,
/// TC payload size, aggregator state growth), updates smoothed EMA estimates,
/// and re-plans remaining chunks if any metric diverges beyond a threshold.
#[derive(Debug, Clone)]
pub struct AdaptivePlanner {
    total_commits: i64,
    memory_budget: i64,
    pipeline_overhead: i64,
    declared_growth: i64,
    /// Growth rate used for the most recent plan.
    current_growth: i64,
    work_ema: Ema,
    tc_ema: Ema,
    agg_ema: Ema,
    replan_threshold: f64,
    replan_count: i32,
}

impl AdaptivePlanner {
    /// Creates an adaptive planner seeded with the declared growth rate.
    #[must_use]
    pub const fn new(
        total_commits: i64,
        mem_budget: i64,
        declared_growth: i64,
        pipeline_overhead: i64,
    ) -> Self {
        Self {
            total_commits,
            memory_budget: mem_budget,
            pipeline_overhead,
            declared_growth,
            current_growth: declared_growth,
            work_ema: Ema::new(DEFAULT_EMA_ALPHA),
            tc_ema: Ema::new(DEFAULT_EMA_ALPHA),
            agg_ema: Ema::new(DEFAULT_EMA_ALPHA),
            replan_threshold: DEFAULT_REPLAN_THRESHOLD,
            replan_count: 0,
        }
    }

    /// Examines three per-chunk metric observations and, if any metric diverges
    /// from prediction by more than the replan threshold, re-computes chunk
    /// boundaries for all chunks after the observed chunk.
    ///
    /// Processed chunks `[0..=chunk_index]` are never modified (checkpoint
    /// safety). The returned slice always covers exactly `[0..total_commits)`.
    #[must_use]
    pub fn replan(&mut self, obs: ReplanObservation) -> Vec<ChunkBounds> {
        // Chunks are well-formed (end >= start); saturating_sub guards against
        // a malformed observation rather than panicking. The non-positive
        // guard reduces to `== 0` because these usize indices are never
        // negative.
        let commits_in_chunk = obs.chunk.end.saturating_sub(obs.chunk.start);
        if commits_in_chunk == 0 {
            return obs.current_chunks;
        }

        // Update all three EMAs with clamped observations.
        let work_val = self
            .work_ema
            .update(obs.work_growth_per_commit.max(MIN_OBSERVED_GROWTH) as f64);
        let tc_val = self
            .tc_ema
            .update(obs.tc_payload_per_commit.max(MIN_OBSERVED_GROWTH) as f64);
        let agg_val = self
            .agg_ema
            .update(obs.agg_growth_per_commit.max(MIN_OBSERVED_GROWTH) as f64);

        // Predicted effective growth rate (with safety margin).
        let mut raw_growth = self.current_growth as f64;
        if raw_growth <= 0.0 {
            raw_growth = DEFAULT_STATE_GROWTH_PER_COMMIT as f64;
        }

        let predicted =
            raw_growth + raw_growth * SAFETY_MARGIN_PERCENT as f64 / PERCENT_DIVISOR as f64;

        // Check divergence for each metric independently. A divergence in any
        // signals instability.
        let triggered = exceeds_threshold(work_val, predicted, self.replan_threshold)
            || exceeds_threshold(tc_val, predicted, self.replan_threshold)
            || exceeds_threshold(agg_val, predicted, self.replan_threshold);

        if !triggered {
            return obs.current_chunks;
        }

        // Use work growth EMA for chunk resizing (TC and agg are informational).
        let new_raw_growth = ((work_val * PERCENT_DIVISOR as f64
            / (PERCENT_DIVISOR + SAFETY_MARGIN_PERCENT) as f64) as i64)
            .max(MIN_OBSERVED_GROWTH);

        self.current_growth = new_raw_growth;
        self.replan_count += 1;

        let planner = self.build_planner(new_raw_growth);
        let tail_chunks = planner.plan_from(obs.chunk.end as i64);

        // Splice: keep processed chunks [0..=chunk_index], append new tail.
        let keep = (obs.chunk_index + 1) as usize;
        let mut result = Vec::with_capacity(keep + tail_chunks.len());
        result.extend_from_slice(&obs.current_chunks[..keep]);
        result.extend(tail_chunks);

        result
    }

    /// Returns adaptive planner telemetry.
    #[must_use]
    pub fn stats(&self) -> AdaptiveStats {
        let final_rate = if self.work_ema.initialized() {
            self.work_ema.value()
        } else {
            self.declared_growth as f64
        };

        AdaptiveStats {
            replan_count: self.replan_count,
            final_growth_rate: final_rate,
            initial_growth_rate: self.declared_growth as f64,
            final_work_growth: self.work_ema.value(),
            final_tc_size: self.tc_ema.value(),
            final_agg_growth: self.agg_ema.value(),
        }
    }

    const fn build_planner(&self, growth: i64) -> Planner {
        Planner {
            total_commits: self.total_commits,
            memory_budget: self.memory_budget,
            aggregate_growth_per_commit: growth,
            pipeline_overhead: self.pipeline_overhead,
        }
    }

    // --- Test-support accessors ---

    /// The initial plan using the declared growth rate.
    #[must_use]
    pub fn initial_plan(&self) -> Vec<ChunkBounds> {
        self.build_planner(self.declared_growth).plan()
    }

    /// Total commits the planner was created with.
    #[must_use]
    pub const fn total_commits(&self) -> i64 {
        self.total_commits
    }

    /// Declared growth rate the planner was seeded with.
    #[must_use]
    pub const fn declared_growth(&self) -> i64 {
        self.declared_growth
    }
}

// ---------------------------------------------------------------------------
// Memory pressure detection
// ---------------------------------------------------------------------------

/// Fraction of budget at which a warning is logged.
pub const PRESSURE_WARNING_RATIO: f64 = 0.80;

/// Fraction of budget at which early hibernation is triggered to prevent OOM
/// before the next chunk starts.
pub const PRESSURE_CRITICAL_RATIO: f64 = 0.90;

/// How close heap usage is to the budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryPressureLevel {
    /// Heap usage is well within budget.
    None,
    /// Heap usage exceeds 80% of budget.
    Warning,
    /// Heap usage exceeds 90% of budget.
    Critical,
}

/// Compares current heap usage against the memory budget and returns the
/// pressure level. Returns [`MemoryPressureLevel::None`] when budget is zero or
/// negative (unlimited).
#[must_use]
pub fn check_memory_pressure(heap_inuse: i64, mem_budget: i64) -> MemoryPressureLevel {
    if mem_budget <= 0 {
        return MemoryPressureLevel::None;
    }

    let ratio = heap_inuse as f64 / mem_budget as f64;

    if ratio >= PRESSURE_CRITICAL_RATIO {
        MemoryPressureLevel::Critical
    } else if ratio >= PRESSURE_WARNING_RATIO {
        MemoryPressureLevel::Warning
    } else {
        MemoryPressureLevel::None
    }
}

// ---------------------------------------------------------------------------
// Budget decomposition / unified scheduler
// ---------------------------------------------------------------------------

/// Fraction of total budget available after slack reserve.
pub const USABLE_PERCENT: i64 = 95;

/// Fraction of remaining budget for analyzer working state.
pub const WORK_STATE_PERCENT: i64 = 60;

/// Fraction of remaining budget for aggregator state.
pub const AGG_STATE_PERCENT: i64 = 30;

/// Fraction of remaining budget for in-flight data.
pub const CHUNK_MEM_PERCENT: i64 = 10;

/// Inputs for the unified budget-aware scheduler.
#[derive(Debug, Clone, Copy, Default)]
pub struct SchedulerConfig {
    /// Number of commits to process.
    pub total_commits: i64,
    /// User-specified memory budget in bytes. Zero means unlimited.
    pub memory_budget: i64,
    /// Estimated fixed overhead for the pipeline (workers, caches, buffers).
    /// When zero, [`BASE_OVERHEAD`] is used.
    pub pipeline_overhead: i64,
    /// Per-commit working state growth in bytes. When zero,
    /// [`DEFAULT_WORKING_STATE_SIZE`] is used.
    pub work_state_per_commit: i64,
    /// Average TC payload size per commit in bytes. Currently informational.
    pub avg_tc_size: i64,
    /// Maximum buffering factor (1=single, 2=double, 3=triple). The scheduler
    /// iterates from this down to 1, selecting the highest factor where
    /// `ChunkSize >= MIN_CHUNK_SIZE`. Zero or negative is treated as 1.
    pub max_buffering: i32,
}

/// Computed scheduling parameters.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Schedule {
    /// Planned chunk boundaries.
    pub chunks: Vec<ChunkBounds>,
    /// Number of commits per chunk (the last chunk may be smaller).
    pub chunk_size: i64,
    /// Pipelining factor (1=single, 2=double, 3=triple).
    pub buffering_factor: i32,
    /// Maximum bytes of aggregator state before spilling. Zero means no limit.
    pub agg_spill_budget: i64,
}

/// Returns `max_buf` clamped to at least 1.
const fn clamp_max_buffering(max_buf: i32) -> i32 {
    if max_buf <= 0 {
        1
    } else {
        max_buf
    }
}

/// Decomposes the memory budget into P + W + A + S regions and computes chunk
/// boundaries, buffering factor, and aggregator spill budget.
///
/// The buffering factor is the highest value in `[1, max_buffering]` for which
/// `chunk_size >= MIN_CHUNK_SIZE`. Only the working-state region is divided
/// among buffering slots; the aggregator spill budget is unaffected.
#[must_use]
pub fn compute_schedule(cfg: SchedulerConfig) -> Schedule {
    let max_buf = clamp_max_buffering(cfg.max_buffering);

    if cfg.total_commits <= 0 {
        return Schedule {
            buffering_factor: 1,
            ..Schedule::default()
        };
    }

    if cfg.memory_budget <= 0 {
        let chunks = Planner {
            total_commits: cfg.total_commits,
            ..Planner::default()
        }
        .plan();

        let chunk_size = chunks
            .first()
            .map_or(MAX_CHUNK_SIZE, |first| (first.end - first.start) as i64);

        return Schedule {
            chunks,
            chunk_size,
            buffering_factor: max_buf,
            agg_spill_budget: 0,
        };
    }

    let usable = cfg.memory_budget * USABLE_PERCENT / PERCENT_DIVISOR;

    let mut overhead = cfg.pipeline_overhead;
    if overhead <= 0 {
        overhead = BASE_OVERHEAD;
    }

    let remaining = usable - overhead;
    if remaining <= 0 {
        let chunks = chunk_i64(cfg.total_commits, MIN_CHUNK_SIZE);

        return Schedule {
            chunks,
            chunk_size: MIN_CHUNK_SIZE,
            buffering_factor: 1,
            agg_spill_budget: 0,
        };
    }

    let work_state = remaining * WORK_STATE_PERCENT / PERCENT_DIVISOR;
    let agg_state = remaining * AGG_STATE_PERCENT / PERCENT_DIVISOR;

    let mut growth = cfg.work_state_per_commit;
    if growth <= 0 {
        growth = DEFAULT_WORKING_STATE_SIZE;
    }

    let effective_growth = growth + growth * SAFETY_MARGIN_PERCENT / PERCENT_DIVISOR;

    // Iterate from max_buf down to 1, selecting the highest factor where
    // chunk_size >= MIN_CHUNK_SIZE. Only work_state is divided among slots.
    let mut chosen_factor = 1;
    let mut chosen_chunk_size = MIN_CHUNK_SIZE;

    let mut bf = max_buf;
    while bf >= 1 {
        let cs = (work_state / (i64::from(bf) * effective_growth)).min(MAX_CHUNK_SIZE);

        if cs >= MIN_CHUNK_SIZE {
            chosen_factor = bf;
            chosen_chunk_size = cs;
            break;
        }

        bf -= 1;
    }

    let chunks = chunk_i64(cfg.total_commits, chosen_chunk_size);

    Schedule {
        chunks,
        chunk_size: chosen_chunk_size,
        buffering_factor: chosen_factor,
        agg_spill_budget: agg_state,
    }
}
