//! Ported from Go `internal/streaming/planner_test.go`.
//!
//! Every Go test function is reproduced one-to-one. The Go `assert.InDelta`
//! becomes an explicit absolute-difference check; `assert.Equal` becomes
//! `assert_eq!`; `require.Greater`/`Len`/etc. become asserts that panic the test
//! the same way.

use cf_streaming::{
    check_memory_pressure, compute_schedule, AdaptivePlanner, ChunkBounds, MemoryPressureLevel,
    Planner, ReplanObservation, SchedulerConfig, AGG_STATE_PERCENT, MAX_CHUNK_SIZE, MIN_CHUNK_SIZE,
    USABLE_PERCENT,
};
use cf_units::{KIB, MIB};

const PERCENT_DIVISOR: i64 = 100;
const SAFETY_MARGIN_PERCENT: i64 = 50;
const WORK_STATE_PERCENT: i64 = 60;
const MIN_OBSERVED_GROWTH: i64 = KIB;

fn in_delta(expected: f64, got: f64, delta: f64) {
    assert!(
        (expected - got).abs() <= delta,
        "expected {expected} within {delta} of {got}"
    );
}

// --- Planner ---

#[test]
fn planner_small_repo_single_chunk() {
    let p = Planner {
        total_commits: 100,
        memory_budget: 2000 * MIB,
        ..Planner::default()
    };
    let chunks = p.plan();
    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0].start, 0);
    assert_eq!(chunks[0].end, 100);
}

#[test]
fn planner_large_repo_multiple_chunks() {
    let p = Planner {
        total_commits: 100_000,
        memory_budget: 2048 * MIB,
        ..Planner::default()
    };
    let chunks = p.plan();
    assert!(chunks.len() > 1);

    assert_eq!(chunks[0].start, 0);
    for i in 1..chunks.len() {
        assert_eq!(chunks[i - 1].end, chunks[i].start);
    }
    assert_eq!(chunks[chunks.len() - 1].end, 100_000);
}

#[test]
fn planner_zero_commits_empty() {
    let p = Planner {
        total_commits: 0,
        memory_budget: 512 * MIB,
        ..Planner::default()
    };
    assert!(p.plan().is_empty());
}

#[test]
fn planner_chunk_size_respects_bounds() {
    let p = Planner {
        total_commits: 100_000,
        memory_budget: 410 * MIB,
        ..Planner::default()
    };
    let chunks = p.plan();
    assert!(!chunks.is_empty());

    for c in &chunks {
        let size = (c.end - c.start) as i64;
        if (c.end as i64) < p.total_commits {
            assert!(size >= MIN_CHUNK_SIZE);
        }
        assert!(size <= MAX_CHUNK_SIZE);
    }
}

#[test]
fn planner_no_budget_uses_max_chunk_size() {
    let p = Planner {
        total_commits: 15_000,
        memory_budget: 0,
        ..Planner::default()
    };
    let chunks = p.plan();
    // 15k / 3000 = 5 chunks.
    assert_eq!(chunks.len(), 5);
}

#[test]
fn planner_aggregate_growth_per_commit() {
    let p = Planner {
        total_commits: 100_000,
        memory_budget: 2048 * MIB,
        aggregate_growth_per_commit: MIB,
        ..Planner::default()
    };
    let chunks = p.plan();
    assert!(!chunks.is_empty());

    let chunk_size = chunks[0].end - chunks[0].start;
    assert_eq!(chunk_size, 1098);
    assert_eq!(chunks.len(), 92);
}

#[test]
fn planner_high_growth_rate_small_chunks() {
    let p = Planner {
        total_commits: 100_000,
        memory_budget: 2048 * MIB,
        aggregate_growth_per_commit: 10 * MIB,
        ..Planner::default()
    };
    let chunks = p.plan();
    let chunk_size = chunks[0].end - chunks[0].start;
    assert_eq!(chunk_size, 109);
}

#[test]
fn planner_low_growth_rate_hits_max_cap() {
    let p = Planner {
        total_commits: 100_000,
        memory_budget: 2048 * MIB,
        aggregate_growth_per_commit: 50 * KIB,
        ..Planner::default()
    };
    let chunks = p.plan();
    let chunk_size = chunks[0].end - chunks[0].start;
    assert_eq!(chunk_size as i64, MAX_CHUNK_SIZE);
}

// --- PlanFrom ---

#[test]
fn plan_from_correct_offsets() {
    let p = Planner {
        total_commits: 10_000,
        memory_budget: 2048 * MIB,
        aggregate_growth_per_commit: MIB,
        ..Planner::default()
    };
    let chunks = p.plan_from(5000);
    assert!(!chunks.is_empty());
    assert_eq!(chunks[0].start, 5000);
    for i in 1..chunks.len() {
        assert_eq!(chunks[i - 1].end, chunks[i].start);
    }
    assert_eq!(chunks[chunks.len() - 1].end, 10_000);
}

#[test]
fn plan_from_at_end_returns_empty() {
    let p = Planner {
        total_commits: 1000,
        memory_budget: 2048 * MIB,
        aggregate_growth_per_commit: MIB,
        ..Planner::default()
    };
    assert!(p.plan_from(1000).is_empty());
    assert!(p.plan_from(2000).is_empty());
}

#[test]
fn plan_from_contiguity_with_original() {
    let p = Planner {
        total_commits: 100_000,
        memory_budget: 2048 * MIB,
        aggregate_growth_per_commit: MIB,
        ..Planner::default()
    };
    let full = p.plan();
    assert!(full.len() > 3);

    let split = full[2].end;
    let tail = p.plan_from(split as i64);
    assert!(!tail.is_empty());
    assert_eq!(tail[0].start, split);
    assert_eq!(tail[tail.len() - 1].end, 100_000);
}

// --- AdaptivePlanner ---

#[test]
fn adaptive_no_replan_when_growth_matches_prediction() {
    let mut ap = AdaptivePlanner::new(10_000, 2048 * MIB, 500 * KIB, 400 * MIB);
    let chunks = ap.initial_plan();
    assert!(chunks.len() > 1);
    let original_len = chunks.len();

    let chunk = chunks[0];
    let predicted = 750 * KIB;
    let new_chunks = ap.replan(ReplanObservation {
        chunk_index: 0,
        chunk,
        work_growth_per_commit: predicted,
        tc_payload_per_commit: predicted,
        agg_growth_per_commit: predicted,
        current_chunks: chunks,
    });

    assert_eq!(new_chunks.len(), original_len);
    assert_eq!(ap.stats().replan_count, 0);
}

#[test]
fn adaptive_replan_when_growth_exceeds_prediction() {
    let mut ap = AdaptivePlanner::new(100_000, 2048 * MIB, 500 * KIB, 400 * MIB);
    let chunks = ap.initial_plan();
    assert!(chunks.len() > 1);
    let original_len = chunks.len();

    let chunk = chunks[0];
    let predicted = 750 * KIB;
    let new_chunks = ap.replan(ReplanObservation {
        chunk_index: 0,
        chunk,
        work_growth_per_commit: 3 * predicted,
        tc_payload_per_commit: predicted,
        agg_growth_per_commit: predicted,
        current_chunks: chunks,
    });

    assert!(new_chunks.len() > original_len);
    assert_eq!(ap.stats().replan_count, 1);
}

#[test]
fn adaptive_replan_when_growth_below_prediction() {
    let mut ap = AdaptivePlanner::new(100_000, 2048 * MIB, 2 * MIB, 400 * MIB);
    let chunks = ap.initial_plan();
    assert!(chunks.len() > 2);
    let original_len = chunks.len();

    let chunk = chunks[0];
    let predicted = 3 * MIB;
    let new_chunks = ap.replan(ReplanObservation {
        chunk_index: 0,
        chunk,
        work_growth_per_commit: 200 * KIB,
        tc_payload_per_commit: predicted,
        agg_growth_per_commit: predicted,
        current_chunks: chunks,
    });

    assert!(new_chunks.len() < original_len);
    assert_eq!(ap.stats().replan_count, 1);
}

#[test]
fn adaptive_preserves_processed_chunks() {
    let mut ap = AdaptivePlanner::new(100_000, 2048 * MIB, 500 * KIB, 400 * MIB);
    let mut chunks = ap.initial_plan();
    assert!(chunks.len() > 5);

    let predicted = 750 * KIB;

    for i in 0..2 {
        let chunk = chunks[i];
        chunks = ap.replan(ReplanObservation {
            chunk_index: i as i32,
            chunk,
            work_growth_per_commit: predicted,
            tc_payload_per_commit: predicted,
            agg_growth_per_commit: predicted,
            current_chunks: chunks,
        });
    }

    let chunk2 = chunks[2];
    let new_chunks = ap.replan(ReplanObservation {
        chunk_index: 2,
        chunk: chunk2,
        work_growth_per_commit: 3 * predicted,
        tc_payload_per_commit: predicted,
        agg_growth_per_commit: predicted,
        current_chunks: chunks.clone(),
    });

    for i in 0..3 {
        assert_eq!(chunks[i], new_chunks[i], "chunk {i} should be preserved");
    }
}

#[test]
fn adaptive_covers_all_commits() {
    const TOTAL_COMMITS: i64 = 50_000;
    let mut ap = AdaptivePlanner::new(TOTAL_COMMITS, 2048 * MIB, 500 * KIB, 400 * MIB);
    let chunks = ap.initial_plan();
    let predicted = 750 * KIB;

    let chunk = chunks[0];
    let new_chunks = ap.replan(ReplanObservation {
        chunk_index: 0,
        chunk,
        work_growth_per_commit: 3 * predicted,
        tc_payload_per_commit: predicted,
        agg_growth_per_commit: predicted,
        current_chunks: chunks,
    });

    assert_eq!(new_chunks[0].start, 0);
    for i in 1..new_chunks.len() {
        assert_eq!(
            new_chunks[i - 1].end,
            new_chunks[i].start,
            "gap between chunk {} and {}",
            i - 1,
            i
        );
    }
    assert_eq!(new_chunks[new_chunks.len() - 1].end, TOTAL_COMMITS as usize);
}

#[test]
fn adaptive_negative_growth_clamped() {
    let mut ap = AdaptivePlanner::new(10_000, 2048 * MIB, 500 * KIB, 400 * MIB);
    let chunks = ap.initial_plan();
    assert!(chunks.len() > 1);

    let chunk = chunks[0];
    let new_chunks = ap.replan(ReplanObservation {
        chunk_index: 0,
        chunk,
        work_growth_per_commit: -400 * KIB,
        tc_payload_per_commit: -100 * KIB,
        agg_growth_per_commit: -200 * KIB,
        current_chunks: chunks,
    });

    assert!(!new_chunks.is_empty());
    assert!(ap.stats().final_growth_rate > 0.0);
}

#[test]
fn adaptive_stats() {
    let ap = AdaptivePlanner::new(10_000, 2048 * MIB, 500 * KIB, 400 * MIB);
    let s = ap.stats();
    assert_eq!(s.replan_count, 0);
    in_delta((500 * KIB) as f64, s.initial_growth_rate, 1.0);
    in_delta((500 * KIB) as f64, s.final_growth_rate, 1.0);
}

#[test]
fn adaptive_accessors() {
    let ap = AdaptivePlanner::new(75_000, 4096 * MIB, MIB, 500 * MIB);
    assert_eq!(ap.total_commits(), 75_000);
    assert_eq!(ap.declared_growth(), MIB);
}

#[test]
fn adaptive_initial_plan_matches_static_planner() {
    const COMMITS: i64 = 100_000;
    const MEM_BUDGET: i64 = 2048 * MIB;
    const GROWTH: i64 = 500 * KIB;
    const OVERHEAD: i64 = 400 * MIB;

    let static_planner = Planner {
        total_commits: COMMITS,
        memory_budget: MEM_BUDGET,
        aggregate_growth_per_commit: GROWTH,
        pipeline_overhead: OVERHEAD,
    };
    let static_chunks = static_planner.plan();

    let ap = AdaptivePlanner::new(COMMITS, MEM_BUDGET, GROWTH, OVERHEAD);
    let adaptive_chunks = ap.initial_plan();

    assert_eq!(static_chunks, adaptive_chunks);
}

#[test]
fn adaptive_ema_smoothing_no_false_replan() {
    let mut ap = AdaptivePlanner::new(100_000, 2048 * MIB, 500 * KIB, 400 * MIB);
    let mut chunks = ap.initial_plan();

    let predicted = 750 * KIB;
    let variations = [0.9_f64, 1.1, 0.95, 1.05, 0.88];

    for i in 0..variations.len().min(chunks.len()) {
        let chunk = chunks[i];
        let observed = (predicted as f64 * variations[i]) as i64;
        chunks = ap.replan(ReplanObservation {
            chunk_index: i as i32,
            chunk,
            work_growth_per_commit: observed,
            tc_payload_per_commit: predicted,
            agg_growth_per_commit: predicted,
            current_chunks: chunks,
        });
    }

    assert_eq!(ap.stats().replan_count, 0);
    in_delta(750.0 * KIB as f64, ap.stats().final_growth_rate, 100.0 * KIB as f64);
}

// --- Memory pressure ---

#[test]
fn check_memory_pressure_none() {
    assert_eq!(
        check_memory_pressure(500 * MIB, 1000 * MIB),
        MemoryPressureLevel::None
    );
}

#[test]
fn check_memory_pressure_warning() {
    assert_eq!(
        check_memory_pressure(850 * MIB, 1000 * MIB),
        MemoryPressureLevel::Warning
    );
}

#[test]
fn check_memory_pressure_critical() {
    assert_eq!(
        check_memory_pressure(950 * MIB, 1000 * MIB),
        MemoryPressureLevel::Critical
    );
}

#[test]
fn check_memory_pressure_exact_warning_boundary() {
    assert_eq!(
        check_memory_pressure(800 * MIB, 1000 * MIB),
        MemoryPressureLevel::Warning
    );
}

#[test]
fn check_memory_pressure_exact_critical_boundary() {
    assert_eq!(
        check_memory_pressure(900 * MIB, 1000 * MIB),
        MemoryPressureLevel::Critical
    );
}

#[test]
fn check_memory_pressure_zero_budget() {
    assert_eq!(
        check_memory_pressure(999 * MIB, 0),
        MemoryPressureLevel::None
    );
}

#[test]
fn check_memory_pressure_negative_budget() {
    assert_eq!(
        check_memory_pressure(999 * MIB, -1),
        MemoryPressureLevel::None
    );
}

// --- ComputeSchedule ---

fn assert_chunks_contiguous(chunks: &[ChunkBounds], total_commits: i64) {
    assert!(!chunks.is_empty());
    assert_eq!(chunks[0].start, 0);
    for i in 1..chunks.len() {
        assert_eq!(
            chunks[i - 1].end,
            chunks[i].start,
            "gap between chunk {} and {}",
            i - 1,
            i
        );
    }
    assert_eq!(chunks[chunks.len() - 1].end, total_commits as usize);
}

#[test]
fn compute_schedule_zero_budget_unlimited() {
    let s = compute_schedule(SchedulerConfig {
        total_commits: 15_000,
        memory_budget: 0,
        ..SchedulerConfig::default()
    });
    assert_eq!(s.chunk_size, MAX_CHUNK_SIZE);
    assert_eq!(s.agg_spill_budget, 0);
    assert_eq!(s.buffering_factor, 1);
    assert_eq!(s.chunks.len(), 5);
}

#[test]
fn compute_schedule_zero_commits_empty() {
    let s = compute_schedule(SchedulerConfig {
        total_commits: 0,
        memory_budget: 2048 * MIB,
        ..SchedulerConfig::default()
    });
    assert!(s.chunks.is_empty());
    assert_eq!(s.buffering_factor, 1);
}

#[test]
fn compute_schedule_512_mib() {
    let s = compute_schedule(SchedulerConfig {
        total_commits: 100_000,
        memory_budget: 512 * MIB,
        pipeline_overhead: 400 * MIB,
        work_state_per_commit: 500 * KIB,
        ..SchedulerConfig::default()
    });
    assert!(s.chunk_size >= MIN_CHUNK_SIZE);
    assert!(s.chunk_size <= MAX_CHUNK_SIZE);
    assert!(s.agg_spill_budget > 0);
    assert_eq!(s.buffering_factor, 1);
    assert_chunks_contiguous(&s.chunks, 100_000);
}

#[test]
fn compute_schedule_2_gib() {
    let s = compute_schedule(SchedulerConfig {
        total_commits: 100_000,
        memory_budget: 2048 * MIB,
        pipeline_overhead: 400 * MIB,
        work_state_per_commit: 500 * KIB,
        ..SchedulerConfig::default()
    });
    assert_eq!(s.chunk_size, 1266);

    let usable = (2048 * MIB) * USABLE_PERCENT / PERCENT_DIVISOR;
    let remaining = usable - 400 * MIB;
    let expected_agg = remaining * AGG_STATE_PERCENT / PERCENT_DIVISOR;
    assert_eq!(s.agg_spill_budget, expected_agg);
    assert_eq!(s.buffering_factor, 1);
    assert_chunks_contiguous(&s.chunks, 100_000);
}

#[test]
fn compute_schedule_4_gib() {
    let s = compute_schedule(SchedulerConfig {
        total_commits: 100_000,
        memory_budget: 4096 * MIB,
        pipeline_overhead: 400 * MIB,
        work_state_per_commit: 500 * KIB,
        ..SchedulerConfig::default()
    });
    assert_eq!(s.chunk_size, 2859);
    assert!(s.agg_spill_budget > 0);
    assert_chunks_contiguous(&s.chunks, 100_000);
}

#[test]
fn compute_schedule_8_gib() {
    let s = compute_schedule(SchedulerConfig {
        total_commits: 100_000,
        memory_budget: 8192 * MIB,
        pipeline_overhead: 400 * MIB,
        work_state_per_commit: 500 * KIB,
        ..SchedulerConfig::default()
    });
    assert_eq!(s.chunk_size, MAX_CHUNK_SIZE);
    assert!(s.agg_spill_budget > 0);
    assert_chunks_contiguous(&s.chunks, 100_000);
}

#[test]
fn compute_schedule_budget_below_overhead() {
    let s = compute_schedule(SchedulerConfig {
        total_commits: 10_000,
        memory_budget: 300 * MIB,
        pipeline_overhead: 400 * MIB,
        work_state_per_commit: 500 * KIB,
        ..SchedulerConfig::default()
    });
    assert_eq!(s.chunk_size, MIN_CHUNK_SIZE);
    assert_eq!(s.agg_spill_budget, 0);
    assert_eq!(s.buffering_factor, 1);
    assert_chunks_contiguous(&s.chunks, 10_000);
}

#[test]
fn compute_schedule_zero_work_state_per_commit_uses_fallback() {
    let s = compute_schedule(SchedulerConfig {
        total_commits: 100_000,
        memory_budget: 2048 * MIB,
        pipeline_overhead: 400 * MIB,
        work_state_per_commit: 0,
        ..SchedulerConfig::default()
    });
    assert_eq!(s.chunk_size, 1582);
    assert!(s.agg_spill_budget > 0);
}

#[test]
fn compute_schedule_zero_pipeline_overhead_uses_fallback() {
    let s = compute_schedule(SchedulerConfig {
        total_commits: 100_000,
        memory_budget: 2048 * MIB,
        pipeline_overhead: 0,
        work_state_per_commit: 500 * KIB,
        ..SchedulerConfig::default()
    });
    assert_eq!(s.chunk_size, 1266);
}

#[test]
fn compute_schedule_agg_spill_budget_proportional() {
    let s = compute_schedule(SchedulerConfig {
        total_commits: 100_000,
        memory_budget: 2048 * MIB,
        pipeline_overhead: 400 * MIB,
        work_state_per_commit: 500 * KIB,
        ..SchedulerConfig::default()
    });
    let usable = (2048 * MIB) * USABLE_PERCENT / PERCENT_DIVISOR;
    let remaining = usable - 400 * MIB;
    let expected_agg = remaining * AGG_STATE_PERCENT / PERCENT_DIVISOR;
    assert_eq!(s.agg_spill_budget, expected_agg);
}

#[test]
fn compute_schedule_single_chunk_small_repo() {
    let s = compute_schedule(SchedulerConfig {
        total_commits: 100,
        memory_budget: 2048 * MIB,
        pipeline_overhead: 400 * MIB,
        work_state_per_commit: 500 * KIB,
        ..SchedulerConfig::default()
    });
    assert_eq!(s.chunks.len(), 1);
    assert_eq!(s.chunks[0].start, 0);
    assert_eq!(s.chunks[0].end, 100);
}

#[test]
fn compute_schedule_buffering_factor_always_one() {
    let budgets = [0_i64, 512 * MIB, 2048 * MIB, 8192 * MIB];
    for b in budgets {
        let s = compute_schedule(SchedulerConfig {
            total_commits: 10_000,
            memory_budget: b,
            ..SchedulerConfig::default()
        });
        assert_eq!(s.buffering_factor, 1, "budget={b}");
    }
}

#[test]
fn compute_schedule_negative_budget_unlimited() {
    let s = compute_schedule(SchedulerConfig {
        total_commits: 15_000,
        memory_budget: -1,
        ..SchedulerConfig::default()
    });
    assert_eq!(s.chunk_size, MAX_CHUNK_SIZE);
    assert_eq!(s.agg_spill_budget, 0);
}

// --- Buffering factor optimization ---

#[test]
fn compute_schedule_8_gib_maxbuf3_double_or_triple() {
    let s = compute_schedule(SchedulerConfig {
        total_commits: 100_000,
        memory_budget: 8192 * MIB,
        pipeline_overhead: 400 * MIB,
        work_state_per_commit: 500 * KIB,
        max_buffering: 3,
        ..SchedulerConfig::default()
    });
    assert!(s.buffering_factor >= 2);
    assert!(s.buffering_factor <= 3);
    assert!(s.chunk_size >= MIN_CHUNK_SIZE);
    assert!(s.chunk_size <= MAX_CHUNK_SIZE);
    assert_chunks_contiguous(&s.chunks, 100_000);
}

#[test]
fn compute_schedule_512_mib_maxbuf3_single_buffer() {
    let s = compute_schedule(SchedulerConfig {
        total_commits: 100_000,
        memory_budget: 512 * MIB,
        pipeline_overhead: 400 * MIB,
        work_state_per_commit: 500 * KIB,
        max_buffering: 3,
        ..SchedulerConfig::default()
    });
    assert_eq!(s.buffering_factor, 1);
    assert!(s.chunk_size >= MIN_CHUNK_SIZE);
    assert_chunks_contiguous(&s.chunks, 100_000);
}

#[test]
fn compute_schedule_4_gib_maxbuf3_double_or_triple() {
    let s = compute_schedule(SchedulerConfig {
        total_commits: 100_000,
        memory_budget: 4096 * MIB,
        pipeline_overhead: 400 * MIB,
        work_state_per_commit: 500 * KIB,
        max_buffering: 3,
        ..SchedulerConfig::default()
    });
    assert!(s.buffering_factor >= 2);
    assert!(s.chunk_size >= MIN_CHUNK_SIZE);
    assert!(s.chunk_size <= MAX_CHUNK_SIZE);
    assert_chunks_contiguous(&s.chunks, 100_000);
}

#[test]
fn compute_schedule_2_gib_maxbuf2_respects_max_cap() {
    let s = compute_schedule(SchedulerConfig {
        total_commits: 100_000,
        memory_budget: 2048 * MIB,
        pipeline_overhead: 400 * MIB,
        work_state_per_commit: 500 * KIB,
        max_buffering: 2,
        ..SchedulerConfig::default()
    });
    assert!(s.buffering_factor <= 2);
    assert!(s.chunk_size >= MIN_CHUNK_SIZE);
    assert_chunks_contiguous(&s.chunks, 100_000);
}

#[test]
fn compute_schedule_unlimited_budget_maxbuf3_uses_max_factor() {
    let s = compute_schedule(SchedulerConfig {
        total_commits: 15_000,
        memory_budget: 0,
        max_buffering: 3,
        ..SchedulerConfig::default()
    });
    assert_eq!(s.buffering_factor, 3);
    assert_eq!(s.chunk_size, MAX_CHUNK_SIZE);
    assert_eq!(s.agg_spill_budget, 0);
}

#[test]
fn compute_schedule_maxbuf_zero_treated_as_one() {
    let s = compute_schedule(SchedulerConfig {
        total_commits: 100_000,
        memory_budget: 2048 * MIB,
        pipeline_overhead: 400 * MIB,
        work_state_per_commit: 500 * KIB,
        max_buffering: 0,
        ..SchedulerConfig::default()
    });
    assert_eq!(s.buffering_factor, 1);
}

#[test]
fn compute_schedule_maxbuf_negative_treated_as_one() {
    let s = compute_schedule(SchedulerConfig {
        total_commits: 100_000,
        memory_budget: 2048 * MIB,
        pipeline_overhead: 400 * MIB,
        work_state_per_commit: 500 * KIB,
        max_buffering: -5,
        ..SchedulerConfig::default()
    });
    assert_eq!(s.buffering_factor, 1);
}

#[test]
fn compute_schedule_maxbuf1_always_single_buffer() {
    let s = compute_schedule(SchedulerConfig {
        total_commits: 100_000,
        memory_budget: 8192 * MIB,
        pipeline_overhead: 400 * MIB,
        work_state_per_commit: 500 * KIB,
        max_buffering: 1,
        ..SchedulerConfig::default()
    });
    assert_eq!(s.buffering_factor, 1);
}

#[test]
fn compute_schedule_agg_spill_budget_invariant() {
    let base = SchedulerConfig {
        total_commits: 100_000,
        memory_budget: 4096 * MIB,
        pipeline_overhead: 400 * MIB,
        work_state_per_commit: 500 * KIB,
        ..SchedulerConfig::default()
    };

    let s1 = compute_schedule(SchedulerConfig {
        max_buffering: 1,
        ..base
    });
    let s2 = compute_schedule(SchedulerConfig {
        max_buffering: 2,
        ..base
    });
    let s3 = compute_schedule(SchedulerConfig {
        max_buffering: 3,
        ..base
    });

    assert_eq!(s1.agg_spill_budget, s2.agg_spill_budget);
    assert_eq!(s2.agg_spill_budget, s3.agg_spill_budget);
}

#[test]
fn compute_schedule_barely_double_buf() {
    let overhead = 400 * MIB;
    let effective_growth = 500 * KIB + (500 * KIB) * SAFETY_MARGIN_PERCENT / PERCENT_DIVISOR;
    let needed_work_state = 2 * MIN_CHUNK_SIZE * effective_growth;
    let remaining = needed_work_state * PERCENT_DIVISOR / WORK_STATE_PERCENT;
    let usable = remaining + overhead;
    let mut mem_budget = usable * PERCENT_DIVISOR / USABLE_PERCENT;
    mem_budget += MIB;

    let s = compute_schedule(SchedulerConfig {
        total_commits: 100_000,
        memory_budget: mem_budget,
        pipeline_overhead: overhead,
        work_state_per_commit: 500 * KIB,
        max_buffering: 3,
        ..SchedulerConfig::default()
    });

    assert_eq!(s.buffering_factor, 2);
    assert!(s.chunk_size >= MIN_CHUNK_SIZE);
}

#[test]
fn compute_schedule_existing_tests_backwards_compatible() {
    let s = compute_schedule(SchedulerConfig {
        total_commits: 100_000,
        memory_budget: 2048 * MIB,
        pipeline_overhead: 400 * MIB,
        work_state_per_commit: 500 * KIB,
        max_buffering: 0,
        ..SchedulerConfig::default()
    });
    assert_eq!(s.chunk_size, 1266);
    assert_eq!(s.buffering_factor, 1);
}

// --- Three-metric adaptive feedback ---

#[test]
fn three_metric_all_match_no_replan() {
    let mut ap = AdaptivePlanner::new(100_000, 2048 * MIB, 500 * KIB, 400 * MIB);
    let chunks = ap.initial_plan();
    assert!(chunks.len() > 1);
    let original_len = chunks.len();

    let predicted = 750 * KIB;
    let chunk = chunks[0];
    let new_chunks = ap.replan(ReplanObservation {
        chunk_index: 0,
        chunk,
        work_growth_per_commit: predicted,
        tc_payload_per_commit: predicted,
        agg_growth_per_commit: predicted,
        current_chunks: chunks,
    });
    assert_eq!(new_chunks.len(), original_len);
    assert_eq!(ap.stats().replan_count, 0);
}

#[test]
fn three_metric_work_growth_high_smaller_chunks() {
    let mut ap = AdaptivePlanner::new(100_000, 2048 * MIB, 500 * KIB, 400 * MIB);
    let chunks = ap.initial_plan();
    let original_len = chunks.len();
    let predicted = 750 * KIB;

    let chunk = chunks[0];
    let new_chunks = ap.replan(ReplanObservation {
        chunk_index: 0,
        chunk,
        work_growth_per_commit: 3 * predicted,
        tc_payload_per_commit: predicted,
        agg_growth_per_commit: predicted,
        current_chunks: chunks,
    });
    assert!(new_chunks.len() > original_len);
    assert_eq!(ap.stats().replan_count, 1);
}

#[test]
fn three_metric_tc_diverges_replan_triggered() {
    let mut ap = AdaptivePlanner::new(100_000, 2048 * MIB, 500 * KIB, 400 * MIB);
    let chunks = ap.initial_plan();
    let predicted = 750 * KIB;

    let chunk = chunks[0];
    let _new_chunks = ap.replan(ReplanObservation {
        chunk_index: 0,
        chunk,
        work_growth_per_commit: predicted,
        tc_payload_per_commit: 3 * predicted,
        agg_growth_per_commit: predicted,
        current_chunks: chunks,
    });
    assert_eq!(ap.stats().replan_count, 1);
}

#[test]
fn three_metric_agg_diverges_replan_triggered() {
    let mut ap = AdaptivePlanner::new(100_000, 2048 * MIB, 500 * KIB, 400 * MIB);
    let chunks = ap.initial_plan();
    let predicted = 750 * KIB;

    let chunk = chunks[0];
    let new_chunks = ap.replan(ReplanObservation {
        chunk_index: 0,
        chunk,
        work_growth_per_commit: predicted,
        tc_payload_per_commit: predicted,
        agg_growth_per_commit: 3 * predicted,
        current_chunks: chunks,
    });
    assert_eq!(ap.stats().replan_count, 1);
    assert!(!new_chunks.is_empty());
}

#[test]
fn three_metric_work_growth_low_larger_chunks() {
    let mut ap = AdaptivePlanner::new(100_000, 2048 * MIB, 2 * MIB, 400 * MIB);
    let chunks = ap.initial_plan();
    let original_len = chunks.len();
    let predicted = 3 * MIB;

    let chunk = chunks[0];
    let new_chunks = ap.replan(ReplanObservation {
        chunk_index: 0,
        chunk,
        work_growth_per_commit: 200 * KIB,
        tc_payload_per_commit: predicted,
        agg_growth_per_commit: predicted,
        current_chunks: chunks,
    });
    assert!(new_chunks.len() < original_len);
    assert_eq!(ap.stats().replan_count, 1);
}

#[test]
fn three_metric_mixed_divergence_replan_triggered() {
    let mut ap = AdaptivePlanner::new(100_000, 2048 * MIB, 500 * KIB, 400 * MIB);
    let chunks = ap.initial_plan();
    let predicted = 750 * KIB;

    let chunk = chunks[0];
    let new_chunks = ap.replan(ReplanObservation {
        chunk_index: 0,
        chunk,
        work_growth_per_commit: predicted,
        tc_payload_per_commit: 3 * predicted,
        agg_growth_per_commit: 3 * predicted,
        current_chunks: chunks,
    });
    assert_eq!(ap.stats().replan_count, 1);
    assert!(!new_chunks.is_empty());
}

#[test]
fn three_metric_all_zero_clamped() {
    let mut ap = AdaptivePlanner::new(10_000, 2048 * MIB, 500 * KIB, 400 * MIB);
    let chunks = ap.initial_plan();
    assert!(chunks.len() > 1);

    let chunk = chunks[0];
    let new_chunks = ap.replan(ReplanObservation {
        chunk_index: 0,
        chunk,
        work_growth_per_commit: 0,
        tc_payload_per_commit: 0,
        agg_growth_per_commit: 0,
        current_chunks: chunks,
    });
    assert!(!new_chunks.is_empty());

    let s = ap.stats();
    in_delta(MIN_OBSERVED_GROWTH as f64, s.final_work_growth, 1.0);
    in_delta(MIN_OBSERVED_GROWTH as f64, s.final_tc_size, 1.0);
    in_delta(MIN_OBSERVED_GROWTH as f64, s.final_agg_growth, 1.0);
}

#[test]
fn three_metric_stats_per_metric_rates() {
    let mut ap = AdaptivePlanner::new(100_000, 2048 * MIB, 500 * KIB, 400 * MIB);
    let chunks = ap.initial_plan();

    let chunk = chunks[0];
    let _ = ap.replan(ReplanObservation {
        chunk_index: 0,
        chunk,
        work_growth_per_commit: 800 * KIB,
        tc_payload_per_commit: 200 * KIB,
        agg_growth_per_commit: 400 * KIB,
        current_chunks: chunks,
    });

    let s = ap.stats();
    in_delta((800 * KIB) as f64, s.final_work_growth, 1.0);
    in_delta((200 * KIB) as f64, s.final_tc_size, 1.0);
    in_delta((400 * KIB) as f64, s.final_agg_growth, 1.0);
    in_delta((800 * KIB) as f64, s.final_growth_rate, 1.0);
}
