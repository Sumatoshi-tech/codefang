package streaming

import (
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func (ap *AdaptivePlanner) InitialPlan() []ChunkBounds {
	return ap.buildPlanner(ap.declaredGrowth).Plan()
}

func (ap *AdaptivePlanner) TotalCommits() int {
	return ap.totalCommits
}

func (ap *AdaptivePlanner) DeclaredGrowth() int64 {
	return ap.declaredGrowth
}

func TestPlanner_SmallRepo_SingleChunk(t *testing.T) {
	t.Parallel()

	// 100 commits fits in a single chunk.
	p := Planner{
		TotalCommits: 100,
		MemoryBudget: 2000 * mib,
	}
	chunks := p.Plan()
	require.Len(t, chunks, 1)
	assert.Equal(t, 0, chunks[0].Start)
	assert.Equal(t, 100, chunks[0].End)
}

func TestPlanner_LargeRepo_MultipleChunks(t *testing.T) {
	t.Parallel()

	// 100k commits with 2GiB budget and default growth (500 KiB/commit).
	// Available for state = 2048MiB - 400MiB overhead = 1648MiB
	// Effective growth = 500 KiB * 1.5 (safety margin) = 750 KiB/commit.
	// Can fit 2250 commits → 100k/2250 = 45 chunks.
	p := Planner{
		TotalCommits: 100000,
		MemoryBudget: 2048 * mib,
	}
	chunks := p.Plan()
	require.Greater(t, len(chunks), 1)

	// Verify chunks are contiguous.
	assert.Equal(t, 0, chunks[0].Start)

	for i := 1; i < len(chunks); i++ {
		assert.Equal(t, chunks[i-1].End, chunks[i].Start)
	}

	assert.Equal(t, 100000, chunks[len(chunks)-1].End)
}

func TestPlanner_ZeroCommits_Empty(t *testing.T) {
	t.Parallel()

	p := Planner{
		TotalCommits: 0,
		MemoryBudget: 512 * mib,
	}
	chunks := p.Plan()
	assert.Empty(t, chunks)
}

func TestPlanner_ChunkSizeRespectsBounds(t *testing.T) {
	t.Parallel()

	// Very tight budget should use MinChunkSize.
	// BaseOverhead=400MiB, so 410MiB leaves 10MiB for state.
	// Effective growth = 500 KiB * 1.5 = 750 KiB → 13 commits → clamped to MinChunkSize=50.
	p := Planner{
		TotalCommits: 100000,
		MemoryBudget: 410 * mib,
	}
	chunks := p.Plan()
	require.NotEmpty(t, chunks)

	// Verify all chunks respect min/max bounds.
	for _, chunk := range chunks {
		size := chunk.End - chunk.Start
		// Last chunk may be smaller, but others should be at least MinChunkSize.
		if chunk.End < p.TotalCommits {
			assert.GreaterOrEqual(t, size, MinChunkSize)
		}

		assert.LessOrEqual(t, size, MaxChunkSize)
	}
}

func TestPlanner_NoBudget_UsesMaxChunkSize(t *testing.T) {
	t.Parallel()

	p := Planner{
		TotalCommits: 15000,
		MemoryBudget: 0, // No budget constraint.
	}
	chunks := p.Plan()
	require.NotEmpty(t, chunks)

	// Without budget, should use MaxChunkSize (3000).
	// 15k commits / 3000 max = 5 chunks.
	assert.Len(t, chunks, 5)
}

func TestPlanner_AggregateGrowthPerCommit(t *testing.T) {
	t.Parallel()

	// 1 MiB/commit declared with 2 GiB budget.
	// Available = 2048 - 400 = 1648 MiB.
	// Effective growth = 1 MiB * 1.5 (safety margin) = 1.5 MiB/commit.
	// At 1.5 MiB/commit → 1098 commits per chunk.
	p := Planner{
		TotalCommits:             100000,
		MemoryBudget:             2048 * mib,
		AggregateGrowthPerCommit: 1 * mib,
	}
	chunks := p.Plan()
	require.NotEmpty(t, chunks)

	// 100k / 1098 ≈ 92 chunks.
	chunkSize := chunks[0].End - chunks[0].Start
	assert.Equal(t, 1098, chunkSize)
	assert.Len(t, chunks, 92)
}

func TestPlanner_HighGrowthRate_SmallChunks(t *testing.T) {
	t.Parallel()

	// 10 MiB/commit declared with 2 GiB budget — simulates all heavy analyzers.
	// Available = 2048 - 400 = 1648 MiB.
	// Effective growth = 10 MiB * 1.5 = 15 MiB/commit → 109 commits per chunk.
	p := Planner{
		TotalCommits:             100000,
		MemoryBudget:             2048 * mib,
		AggregateGrowthPerCommit: 10 * mib,
	}
	chunks := p.Plan()
	require.NotEmpty(t, chunks)

	chunkSize := chunks[0].End - chunks[0].Start
	assert.Equal(t, 109, chunkSize)
}

func TestPlanner_LowGrowthRate_HitsMaxCap(t *testing.T) {
	t.Parallel()

	// 50 KiB/commit (e.g. devs only) with 2 GiB budget.
	// Available = 1648 MiB. Effective = 75 KiB → 22489 commits → capped to MaxChunkSize=3000.
	p := Planner{
		TotalCommits:             100000,
		MemoryBudget:             2048 * mib,
		AggregateGrowthPerCommit: 50 * kib,
	}
	chunks := p.Plan()
	require.NotEmpty(t, chunks)

	chunkSize := chunks[0].End - chunks[0].Start
	assert.Equal(t, MaxChunkSize, chunkSize)
}

func TestEMA_InitialValue(t *testing.T) {
	t.Parallel()

	var ema emaGrowthRate

	got := ema.Update(500.0, 0.3)
	assert.InDelta(t, 500.0, got, 0.01)
}

func TestEMA_ConvergesToStableInput(t *testing.T) {
	t.Parallel()

	var ema emaGrowthRate

	for range 20 {
		ema.Update(1000.0, 0.3)
	}

	assert.InDelta(t, 1000.0, ema.value, 1.0)
}

func TestEMA_RespondsToSpike(t *testing.T) {
	t.Parallel()

	var ema emaGrowthRate

	// Establish a baseline at 100.
	for range 10 {
		ema.Update(100.0, 0.3)
	}

	// Single spike at 1000.
	spiked := ema.Update(1000.0, 0.3)

	// EMA should move towards the spike but not match it.
	assert.Greater(t, spiked, 100.0)
	assert.Less(t, spiked, 1000.0)
	// With alpha=0.3: new = 0.3*1000 + 0.7*~100 = ~370.
	assert.InDelta(t, 370.0, spiked, 5.0)
}

func TestPlanFrom_CorrectOffsets(t *testing.T) {
	t.Parallel()

	p := Planner{
		TotalCommits:             10000,
		MemoryBudget:             2048 * mib,
		AggregateGrowthPerCommit: 1 * mib,
	}

	chunks := p.PlanFrom(5000)
	require.NotEmpty(t, chunks)

	// First chunk should start at 5000.
	assert.Equal(t, 5000, chunks[0].Start)

	// Chunks should be contiguous.
	for i := 1; i < len(chunks); i++ {
		assert.Equal(t, chunks[i-1].End, chunks[i].Start)
	}

	// Last chunk should end at TotalCommits.
	assert.Equal(t, 10000, chunks[len(chunks)-1].End)
}

func TestPlanFrom_AtEnd_ReturnsNil(t *testing.T) {
	t.Parallel()

	p := Planner{
		TotalCommits:             1000,
		MemoryBudget:             2048 * mib,
		AggregateGrowthPerCommit: 1 * mib,
	}

	assert.Nil(t, p.PlanFrom(1000))
	assert.Nil(t, p.PlanFrom(2000))
}

func TestPlanFrom_ContiguityWithOriginal(t *testing.T) {
	t.Parallel()

	p := Planner{
		TotalCommits:             100000,
		MemoryBudget:             2048 * mib,
		AggregateGrowthPerCommit: 1 * mib,
	}

	fullChunks := p.Plan()
	require.Greater(t, len(fullChunks), 3)

	// Split at chunk 2's end and plan remainder.
	splitPoint := fullChunks[2].End
	tailChunks := p.PlanFrom(splitPoint)

	require.NotEmpty(t, tailChunks)
	assert.Equal(t, splitPoint, tailChunks[0].Start)
	assert.Equal(t, 100000, tailChunks[len(tailChunks)-1].End)
}

func TestAdaptivePlanner_NoReplan_WhenGrowthMatchesPrediction(t *testing.T) {
	t.Parallel()

	ap := NewAdaptivePlanner(10000, 2048*mib, 500*kib, 400*mib)
	chunks := ap.InitialPlan()
	require.Greater(t, len(chunks), 1)

	originalLen := len(chunks)

	// Simulate observed growth matching predicted (500 KiB * 1.5 safety = 750 KiB effective).
	// All three metrics match predicted — no replan expected.
	chunk := chunks[0]
	predicted := int64(750 * kib)

	newChunks := ap.Replan(ReplanObservation{
		ChunkIndex:          0,
		Chunk:               chunk,
		WorkGrowthPerCommit: predicted,
		TCPayloadPerCommit:  predicted,
		AggGrowthPerCommit:  predicted,
		CurrentChunks:       chunks,
	})

	assert.Len(t, newChunks, originalLen)
	assert.Equal(t, 0, ap.Stats().ReplanCount)
}

func TestAdaptivePlanner_Replan_WhenGrowthExceedsPrediction(t *testing.T) {
	t.Parallel()

	ap := NewAdaptivePlanner(100000, 2048*mib, 500*kib, 400*mib)
	chunks := ap.InitialPlan()
	require.Greater(t, len(chunks), 1)

	originalLen := len(chunks)

	// Simulate work growth 3x predicted — well above 25% threshold.
	chunk := chunks[0]
	predicted := int64(750 * kib)

	newChunks := ap.Replan(ReplanObservation{
		ChunkIndex:          0,
		Chunk:               chunk,
		WorkGrowthPerCommit: 3 * predicted,
		TCPayloadPerCommit:  predicted,
		AggGrowthPerCommit:  predicted,
		CurrentChunks:       chunks,
	})

	// Should have replanned with smaller chunks (more total chunks).
	assert.Greater(t, len(newChunks), originalLen)
	assert.Equal(t, 1, ap.Stats().ReplanCount)
}

func TestAdaptivePlanner_Replan_WhenGrowthBelowPrediction(t *testing.T) {
	t.Parallel()

	// Use high declared growth so there's room to make chunks bigger.
	ap := NewAdaptivePlanner(100000, 2048*mib, 2*mib, 400*mib)
	chunks := ap.InitialPlan()
	require.Greater(t, len(chunks), 2)

	originalLen := len(chunks)

	// Simulate observed growth much lower than declared.
	// Predicted effective = 2 MiB * 1.5 = 3 MiB. Observed: 200 KiB.
	chunk := chunks[0]
	predicted := int64(3 * mib)

	newChunks := ap.Replan(ReplanObservation{
		ChunkIndex:          0,
		Chunk:               chunk,
		WorkGrowthPerCommit: 200 * kib,
		TCPayloadPerCommit:  predicted,
		AggGrowthPerCommit:  predicted,
		CurrentChunks:       chunks,
	})

	// Should have replanned with larger chunks (fewer total chunks).
	assert.Less(t, len(newChunks), originalLen)
	assert.Equal(t, 1, ap.Stats().ReplanCount)
}

func TestAdaptivePlanner_PreservesProcessedChunks(t *testing.T) {
	t.Parallel()

	ap := NewAdaptivePlanner(100000, 2048*mib, 500*kib, 400*mib)
	chunks := ap.InitialPlan()
	require.Greater(t, len(chunks), 5)

	predicted := int64(750 * kib)

	// Process chunks 0-1 with matching growth (no replan).
	for i := range 2 {
		chunk := chunks[i]
		chunks = ap.Replan(ReplanObservation{
			ChunkIndex:          i,
			Chunk:               chunk,
			WorkGrowthPerCommit: predicted,
			TCPayloadPerCommit:  predicted,
			AggGrowthPerCommit:  predicted,
			CurrentChunks:       chunks,
		})
	}

	// Now force a replan at chunk 2 with 3x work growth.
	chunk2 := chunks[2]

	newChunks := ap.Replan(ReplanObservation{
		ChunkIndex:          2,
		Chunk:               chunk2,
		WorkGrowthPerCommit: 3 * predicted,
		TCPayloadPerCommit:  predicted,
		AggGrowthPerCommit:  predicted,
		CurrentChunks:       chunks,
	})

	// Chunks 0, 1, 2 should be identical.
	for i := range 3 {
		assert.Equal(t, chunks[i], newChunks[i], "chunk %d should be preserved", i)
	}
}

func TestAdaptivePlanner_CoversAllCommits(t *testing.T) {
	t.Parallel()

	const totalCommits = 50000

	ap := NewAdaptivePlanner(totalCommits, 2048*mib, 500*kib, 400*mib)
	chunks := ap.InitialPlan()
	predicted := int64(750 * kib)

	// Force a replan with 3x work growth.
	chunk := chunks[0]

	newChunks := ap.Replan(ReplanObservation{
		ChunkIndex:          0,
		Chunk:               chunk,
		WorkGrowthPerCommit: 3 * predicted,
		TCPayloadPerCommit:  predicted,
		AggGrowthPerCommit:  predicted,
		CurrentChunks:       chunks,
	})

	// Verify contiguity and full coverage.
	assert.Equal(t, 0, newChunks[0].Start)

	for i := 1; i < len(newChunks); i++ {
		assert.Equal(t, newChunks[i-1].End, newChunks[i].Start, "gap between chunk %d and %d", i-1, i)
	}

	assert.Equal(t, totalCommits, newChunks[len(newChunks)-1].End)
}

func TestAdaptivePlanner_NegativeGrowthClamped(t *testing.T) {
	t.Parallel()

	ap := NewAdaptivePlanner(10000, 2048*mib, 500*kib, 400*mib)
	chunks := ap.InitialPlan()
	require.Greater(t, len(chunks), 1)

	chunk := chunks[0]

	// Simulate negative growth (hibernation freed more than allocated).
	// All metrics negative → clamped to minObservedGrowth inside Replan.
	newChunks := ap.Replan(ReplanObservation{
		ChunkIndex:          0,
		Chunk:               chunk,
		WorkGrowthPerCommit: -400 * kib,
		TCPayloadPerCommit:  -100 * kib,
		AggGrowthPerCommit:  -200 * kib,
		CurrentChunks:       chunks,
	})

	// Should still produce valid chunks (clamped to floor).
	assert.NotEmpty(t, newChunks)

	// EMA should be positive (clamped to minObservedGrowth).
	stats := ap.Stats()
	assert.Greater(t, stats.FinalGrowthRate, 0.0)
}

func TestAdaptivePlanner_Stats(t *testing.T) {
	t.Parallel()

	ap := NewAdaptivePlanner(10000, 2048*mib, 500*kib, 400*mib)

	stats := ap.Stats()
	assert.Equal(t, 0, stats.ReplanCount)
	assert.InDelta(t, float64(500*kib), stats.InitialGrowthRate, 1.0)
	assert.InDelta(t, float64(500*kib), stats.FinalGrowthRate, 1.0) // No EMA yet, uses declared.
}

func TestHeapSnapshot_ReturnsPositiveValues(t *testing.T) {
	t.Parallel()

	snap := TakeHeapSnapshot()
	assert.Positive(t, snap.HeapInuse)
	assert.Positive(t, snap.HeapAlloc)
	assert.Positive(t, snap.TakenAtNS)
}

func TestEMA_AlphaOne_TracksLatest(t *testing.T) {
	t.Parallel()

	var ema emaGrowthRate
	ema.Update(100.0, 1.0)
	got := ema.Update(500.0, 1.0)

	assert.InDelta(t, 500.0, got, 0.01)
}

func TestEMA_AlphaZero_KeepsInitial(t *testing.T) {
	t.Parallel()

	var ema emaGrowthRate
	ema.Update(100.0, 0.0)
	got := ema.Update(500.0, 0.0)

	// Alpha=0 means "trust only history": new = 0*500 + 1*100 = 100.
	assert.InDelta(t, 100.0, got, 0.01)
}

func TestAdaptivePlanner_Accessors(t *testing.T) {
	t.Parallel()

	ap := NewAdaptivePlanner(75000, 4096*mib, 1*mib, 500*mib)
	assert.Equal(t, 75000, ap.TotalCommits())
	assert.Equal(t, int64(1*mib), ap.DeclaredGrowth())
}

func TestAdaptivePlanner_InitialPlanMatchesStaticPlanner(t *testing.T) {
	t.Parallel()

	const (
		commits  = 100000
		budget   = int64(2048 * mib)
		growth   = int64(500 * kib)
		overhead = int64(400 * mib)
	)

	// Static planner.
	staticPlanner := Planner{
		TotalCommits:             commits,
		MemoryBudget:             budget,
		AggregateGrowthPerCommit: growth,
		PipelineOverhead:         overhead,
	}
	staticChunks := staticPlanner.Plan()

	// Adaptive planner initial plan.
	ap := NewAdaptivePlanner(commits, budget, growth, overhead)
	adaptiveChunks := ap.InitialPlan()

	assert.Equal(t, staticChunks, adaptiveChunks)
}

// Verify that repeated within-threshold observations don't trigger replans.
func TestAdaptivePlanner_EMASmoothing_NoFalseReplan(t *testing.T) {
	t.Parallel()

	ap := NewAdaptivePlanner(100000, 2048*mib, 500*kib, 400*mib)
	chunks := ap.InitialPlan()

	// Process 5 chunks with growth within 20% of effective growth (750 KiB).
	// All within the 25% threshold — no replan expected.
	predicted := int64(750 * kib)
	variations := []float64{0.9, 1.1, 0.95, 1.05, 0.88}

	for i := 0; i < len(variations) && i < len(chunks); i++ {
		chunk := chunks[i]
		observed := int64(float64(predicted) * variations[i])

		chunks = ap.Replan(ReplanObservation{
			ChunkIndex:          i,
			Chunk:               chunk,
			WorkGrowthPerCommit: observed,
			TCPayloadPerCommit:  predicted,
			AggGrowthPerCommit:  predicted,
			CurrentChunks:       chunks,
		})
	}

	assert.Equal(t, 0, ap.Stats().ReplanCount)

	// EMA should be close to effective growth (750 KiB).
	assert.InDelta(t, 750.0*float64(kib), ap.Stats().FinalGrowthRate, 100.0*float64(kib))
}

func TestCheckMemoryPressure_None(t *testing.T) {
	t.Parallel()

	assert.Equal(t, PressureNone, CheckMemoryPressure(500*mib, 1000*mib))
}

func TestCheckMemoryPressure_Warning(t *testing.T) {
	t.Parallel()

	assert.Equal(t, PressureWarning, CheckMemoryPressure(850*mib, 1000*mib))
}

func TestCheckMemoryPressure_Critical(t *testing.T) {
	t.Parallel()

	assert.Equal(t, PressureCritical, CheckMemoryPressure(950*mib, 1000*mib))
}

func TestCheckMemoryPressure_ExactWarningBoundary(t *testing.T) {
	t.Parallel()

	assert.Equal(t, PressureWarning, CheckMemoryPressure(800*mib, 1000*mib))
}

func TestCheckMemoryPressure_ExactCriticalBoundary(t *testing.T) {
	t.Parallel()

	assert.Equal(t, PressureCritical, CheckMemoryPressure(900*mib, 1000*mib))
}

func TestCheckMemoryPressure_ZeroBudget(t *testing.T) {
	t.Parallel()

	assert.Equal(t, PressureNone, CheckMemoryPressure(999*mib, 0))
}

func TestCheckMemoryPressure_NegativeBudget(t *testing.T) {
	t.Parallel()

	assert.Equal(t, PressureNone, CheckMemoryPressure(999*mib, -1))
}

// ComputeSchedule tests.

func TestComputeSchedule_ZeroBudget_Unlimited(t *testing.T) {
	t.Parallel()

	s := ComputeSchedule(SchedulerConfig{
		TotalCommits: 15000,
		MemoryBudget: 0,
	})

	// Unlimited budget → MaxChunkSize, no spill.
	assert.Equal(t, MaxChunkSize, s.ChunkSize)
	assert.Equal(t, int64(0), s.AggSpillBudget)
	assert.Equal(t, 1, s.BufferingFactor)
	assert.Len(t, s.Chunks, 5)
}

func TestComputeSchedule_ZeroCommits_Empty(t *testing.T) {
	t.Parallel()

	s := ComputeSchedule(SchedulerConfig{
		TotalCommits: 0,
		MemoryBudget: 2048 * mib,
	})

	assert.Empty(t, s.Chunks)
	assert.Equal(t, 1, s.BufferingFactor)
}

func TestComputeSchedule_512MiB(t *testing.T) {
	t.Parallel()

	s := ComputeSchedule(SchedulerConfig{
		TotalCommits:       100000,
		MemoryBudget:       512 * mib,
		PipelineOverhead:   400 * mib,
		WorkStatePerCommit: 500 * kib,
	})

	// usable = 512 * 0.95 = 486 MiB
	// remaining = 486 - 400 = 86 MiB
	// workState = 86 * 0.60 = ~51 MiB
	// aggState = 86 * 0.30 = ~25 MiB
	// effectiveGrowth = 500 KiB * 1.5 = 750 KiB
	// chunkSize = 51 MiB / 750 KiB = ~70 commits.
	assert.GreaterOrEqual(t, s.ChunkSize, MinChunkSize)
	assert.LessOrEqual(t, s.ChunkSize, MaxChunkSize)
	assert.Positive(t, s.AggSpillBudget)
	assert.Equal(t, 1, s.BufferingFactor)

	// Verify contiguity.
	assertChunksContiguous(t, s.Chunks, 100000)
}

func TestComputeSchedule_2GiB(t *testing.T) {
	t.Parallel()

	s := ComputeSchedule(SchedulerConfig{
		TotalCommits:       100000,
		MemoryBudget:       2048 * mib,
		PipelineOverhead:   400 * mib,
		WorkStatePerCommit: 500 * kib,
	})

	// usable = 2048*mib * 95/100 = 2040109465
	// remaining = 2040109465 - 400*mib = 1620679065
	// workState = 1620679065 * 60/100 = 972407439
	// aggState = 1620679065 * 30/100 = 486203719
	// effectiveGrowth = 500*kib * 1.5 = 768000
	// chunkSize = 972407439 / 768000 = 1266.
	assert.Equal(t, 1266, s.ChunkSize)

	usable := int64(2048*mib) * UsablePercent / percentDivisor
	remaining := usable - 400*mib
	expectedAgg := remaining * AggStatePercent / percentDivisor
	assert.Equal(t, expectedAgg, s.AggSpillBudget)
	assert.Equal(t, 1, s.BufferingFactor)

	assertChunksContiguous(t, s.Chunks, 100000)
}

func TestComputeSchedule_4GiB(t *testing.T) {
	t.Parallel()

	s := ComputeSchedule(SchedulerConfig{
		TotalCommits:       100000,
		MemoryBudget:       4096 * mib,
		PipelineOverhead:   400 * mib,
		WorkStatePerCommit: 500 * kib,
	})

	// usable = 4096 * 0.95 = 3891 MiB
	// remaining = 3891 - 400 = 3491 MiB
	// workState = 3491 * 0.60 = 2094 MiB
	// effectiveGrowth = 750 KiB
	// chunkSize = 2094 MiB / 750 KiB = 2859.
	assert.Equal(t, 2859, s.ChunkSize)
	assert.Positive(t, s.AggSpillBudget)

	assertChunksContiguous(t, s.Chunks, 100000)
}

func TestComputeSchedule_8GiB(t *testing.T) {
	t.Parallel()

	s := ComputeSchedule(SchedulerConfig{
		TotalCommits:       100000,
		MemoryBudget:       8192 * mib,
		PipelineOverhead:   400 * mib,
		WorkStatePerCommit: 500 * kib,
	})

	// usable = 8192 * 0.95 = 7782 MiB
	// remaining = 7782 - 400 = 7382 MiB
	// workState = 7382 * 0.60 = 4429 MiB
	// effectiveGrowth = 750 KiB
	// chunkSize = 4429 MiB / 750 KiB = 6046, capped to MaxChunkSize.
	assert.Equal(t, MaxChunkSize, s.ChunkSize)
	assert.Positive(t, s.AggSpillBudget)

	assertChunksContiguous(t, s.Chunks, 100000)
}

func TestComputeSchedule_BudgetBelowOverhead(t *testing.T) {
	t.Parallel()

	s := ComputeSchedule(SchedulerConfig{
		TotalCommits:       10000,
		MemoryBudget:       300 * mib,
		PipelineOverhead:   400 * mib,
		WorkStatePerCommit: 500 * kib,
	})

	// usable = 300 * 0.95 = 285 MiB < 400 MiB overhead → remaining <= 0.
	assert.Equal(t, MinChunkSize, s.ChunkSize)
	assert.Equal(t, int64(0), s.AggSpillBudget)
	assert.Equal(t, 1, s.BufferingFactor)

	assertChunksContiguous(t, s.Chunks, 10000)
}

func TestComputeSchedule_ZeroWorkStatePerCommit_UsesFallback(t *testing.T) {
	t.Parallel()

	s := ComputeSchedule(SchedulerConfig{
		TotalCommits:       100000,
		MemoryBudget:       2048 * mib,
		PipelineOverhead:   400 * mib,
		WorkStatePerCommit: 0, // Should use DefaultWorkingStateSize (400 KiB).
	})

	// DefaultWorkingStateSize = 400 KiB, effective = 600 KiB = 614400.
	// remaining = 1620679065, workState = 972407439.
	// chunkSize = 972407439 / 614400 = 1582.
	assert.Equal(t, 1582, s.ChunkSize)
	assert.Positive(t, s.AggSpillBudget)
}

func TestComputeSchedule_ZeroPipelineOverhead_UsesFallback(t *testing.T) {
	t.Parallel()

	s := ComputeSchedule(SchedulerConfig{
		TotalCommits:       100000,
		MemoryBudget:       2048 * mib,
		PipelineOverhead:   0, // Should use BaseOverhead (400 MiB).
		WorkStatePerCommit: 500 * kib,
	})

	// Same result as the 2 GiB test (1266).
	assert.Equal(t, 1266, s.ChunkSize)
}

func TestComputeSchedule_AggSpillBudgetProportional(t *testing.T) {
	t.Parallel()

	s := ComputeSchedule(SchedulerConfig{
		TotalCommits:       100000,
		MemoryBudget:       2048 * mib,
		PipelineOverhead:   400 * mib,
		WorkStatePerCommit: 500 * kib,
	})

	// remaining = usable - overhead = 1945 - 400 = 1545 MiB.
	// aggState = remaining * 30%.
	usable := int64(2048*mib) * UsablePercent / percentDivisor
	remaining := usable - 400*mib
	expectedAgg := remaining * AggStatePercent / percentDivisor

	assert.Equal(t, expectedAgg, s.AggSpillBudget)
}

func TestComputeSchedule_SingleChunk_SmallRepo(t *testing.T) {
	t.Parallel()

	s := ComputeSchedule(SchedulerConfig{
		TotalCommits:       100,
		MemoryBudget:       2048 * mib,
		PipelineOverhead:   400 * mib,
		WorkStatePerCommit: 500 * kib,
	})

	// 100 commits < chunkSize of 1265 → single chunk.
	require.Len(t, s.Chunks, 1)
	assert.Equal(t, 0, s.Chunks[0].Start)
	assert.Equal(t, 100, s.Chunks[0].End)
}

func TestComputeSchedule_BufferingFactorAlwaysOne(t *testing.T) {
	t.Parallel()

	budgets := []int64{0, 512 * mib, 2048 * mib, 8192 * mib}
	for _, b := range budgets {
		s := ComputeSchedule(SchedulerConfig{
			TotalCommits: 10000,
			MemoryBudget: b,
		})
		assert.Equal(t, 1, s.BufferingFactor, "budget=%d", b)
	}
}

func TestComputeSchedule_NegativeBudget_Unlimited(t *testing.T) {
	t.Parallel()

	s := ComputeSchedule(SchedulerConfig{
		TotalCommits: 15000,
		MemoryBudget: -1,
	})

	assert.Equal(t, MaxChunkSize, s.ChunkSize)
	assert.Equal(t, int64(0), s.AggSpillBudget)
}

// Buffering factor optimization tests (4.2).

func TestComputeSchedule_8GiB_MaxBuf3_DoubleOrTriple(t *testing.T) {
	t.Parallel()

	s := ComputeSchedule(SchedulerConfig{
		TotalCommits:       100000,
		MemoryBudget:       8192 * mib,
		PipelineOverhead:   400 * mib,
		WorkStatePerCommit: 500 * kib,
		MaxBuffering:       3,
	})

	// 8 GiB: workState is large enough for factor >= 2.
	assert.GreaterOrEqual(t, s.BufferingFactor, 2)
	assert.LessOrEqual(t, s.BufferingFactor, 3)
	assert.GreaterOrEqual(t, s.ChunkSize, MinChunkSize)
	assert.LessOrEqual(t, s.ChunkSize, MaxChunkSize)

	assertChunksContiguous(t, s.Chunks, 100000)
}

func TestComputeSchedule_512MiB_MaxBuf3_SingleBuffer(t *testing.T) {
	t.Parallel()

	s := ComputeSchedule(SchedulerConfig{
		TotalCommits:       100000,
		MemoryBudget:       512 * mib,
		PipelineOverhead:   400 * mib,
		WorkStatePerCommit: 500 * kib,
		MaxBuffering:       3,
	})

	// 512 MiB is very tight — should fall back to factor=1.
	assert.Equal(t, 1, s.BufferingFactor)
	assert.GreaterOrEqual(t, s.ChunkSize, MinChunkSize)

	assertChunksContiguous(t, s.Chunks, 100000)
}

func TestComputeSchedule_4GiB_MaxBuf3_DoubleOrTriple(t *testing.T) {
	t.Parallel()

	s := ComputeSchedule(SchedulerConfig{
		TotalCommits:       100000,
		MemoryBudget:       4096 * mib,
		PipelineOverhead:   400 * mib,
		WorkStatePerCommit: 500 * kib,
		MaxBuffering:       3,
	})

	assert.GreaterOrEqual(t, s.BufferingFactor, 2)
	assert.GreaterOrEqual(t, s.ChunkSize, MinChunkSize)
	assert.LessOrEqual(t, s.ChunkSize, MaxChunkSize)

	assertChunksContiguous(t, s.Chunks, 100000)
}

func TestComputeSchedule_2GiB_MaxBuf2_RespectsMaxCap(t *testing.T) {
	t.Parallel()

	s := ComputeSchedule(SchedulerConfig{
		TotalCommits:       100000,
		MemoryBudget:       2048 * mib,
		PipelineOverhead:   400 * mib,
		WorkStatePerCommit: 500 * kib,
		MaxBuffering:       2,
	})

	// MaxBuf=2 caps the factor at 2.
	assert.LessOrEqual(t, s.BufferingFactor, 2)
	assert.GreaterOrEqual(t, s.ChunkSize, MinChunkSize)

	assertChunksContiguous(t, s.Chunks, 100000)
}

func TestComputeSchedule_UnlimitedBudget_MaxBuf3_UsesMaxFactor(t *testing.T) {
	t.Parallel()

	s := ComputeSchedule(SchedulerConfig{
		TotalCommits: 15000,
		MemoryBudget: 0,
		MaxBuffering: 3,
	})

	// Unlimited budget → max parallelism.
	assert.Equal(t, 3, s.BufferingFactor)
	assert.Equal(t, MaxChunkSize, s.ChunkSize)
	assert.Equal(t, int64(0), s.AggSpillBudget)
}

func TestComputeSchedule_MaxBufZero_TreatedAsOne(t *testing.T) {
	t.Parallel()

	s := ComputeSchedule(SchedulerConfig{
		TotalCommits:       100000,
		MemoryBudget:       2048 * mib,
		PipelineOverhead:   400 * mib,
		WorkStatePerCommit: 500 * kib,
		MaxBuffering:       0,
	})

	assert.Equal(t, 1, s.BufferingFactor)
}

func TestComputeSchedule_MaxBufNegative_TreatedAsOne(t *testing.T) {
	t.Parallel()

	s := ComputeSchedule(SchedulerConfig{
		TotalCommits:       100000,
		MemoryBudget:       2048 * mib,
		PipelineOverhead:   400 * mib,
		WorkStatePerCommit: 500 * kib,
		MaxBuffering:       -5,
	})

	assert.Equal(t, 1, s.BufferingFactor)
}

func TestComputeSchedule_MaxBuf1_AlwaysSingleBuffer(t *testing.T) {
	t.Parallel()

	s := ComputeSchedule(SchedulerConfig{
		TotalCommits:       100000,
		MemoryBudget:       8192 * mib,
		PipelineOverhead:   400 * mib,
		WorkStatePerCommit: 500 * kib,
		MaxBuffering:       1,
	})

	// Even with huge budget, MaxBuf=1 forces single-buffering.
	assert.Equal(t, 1, s.BufferingFactor)
}

func TestComputeSchedule_AggSpillBudgetInvariant(t *testing.T) {
	t.Parallel()

	// AggSpillBudget should be the same regardless of MaxBuffering.
	cfg := SchedulerConfig{
		TotalCommits:       100000,
		MemoryBudget:       4096 * mib,
		PipelineOverhead:   400 * mib,
		WorkStatePerCommit: 500 * kib,
	}

	cfg.MaxBuffering = 1
	s1 := ComputeSchedule(cfg)

	cfg.MaxBuffering = 2
	s2 := ComputeSchedule(cfg)

	cfg.MaxBuffering = 3
	s3 := ComputeSchedule(cfg)

	assert.Equal(t, s1.AggSpillBudget, s2.AggSpillBudget)
	assert.Equal(t, s2.AggSpillBudget, s3.AggSpillBudget)
}

func TestComputeSchedule_BarelyDoubleBuf(t *testing.T) {
	t.Parallel()

	// Find a budget where factor=2 produces chunkSize just at MinChunkSize.
	// workState / (2 * effectiveGrowth) = MinChunkSize
	// workState = 2 * MinChunkSize * effectiveGrowth
	// With WorkState=500 KiB, effectiveGrowth = 750 KiB = 768000.
	// workState needed = 2 * 50 * 768000 = 76800000 bytes.
	// workState = remaining * 60/100 → remaining = 76800000 * 100/60 = 128000000.
	// remaining = usable - overhead → usable = 128000000 + 400*mib.
	// usable = budget * 95/100 → budget = usable * 100/95.
	overhead := int64(400 * mib)
	effectiveGrowth := int64(500*kib) + int64(500*kib)*safetyMarginPercent/percentDivisor
	neededWorkState := int64(2) * int64(MinChunkSize) * effectiveGrowth
	remaining := neededWorkState * percentDivisor / WorkStatePercent
	usable := remaining + overhead
	budget := usable * percentDivisor / UsablePercent

	// Add a small margin to ensure we're above the threshold.
	budget += 1 * mib

	s := ComputeSchedule(SchedulerConfig{
		TotalCommits:       100000,
		MemoryBudget:       budget,
		PipelineOverhead:   overhead,
		WorkStatePerCommit: 500 * kib,
		MaxBuffering:       3,
	})

	// Should select factor=2 (factor=3 would produce chunkSize < MinChunkSize).
	assert.Equal(t, 2, s.BufferingFactor)
	assert.GreaterOrEqual(t, s.ChunkSize, MinChunkSize)
}

func TestComputeSchedule_ExistingTests_BackwardsCompatible(t *testing.T) {
	t.Parallel()

	// Existing tests used MaxBuffering=0 (or unset). With the change, MaxBuf=0 is
	// treated as 1. The old behavior was hardcoded BufferingFactor=1, so
	// chunk sizes should be identical.
	s := ComputeSchedule(SchedulerConfig{
		TotalCommits:       100000,
		MemoryBudget:       2048 * mib,
		PipelineOverhead:   400 * mib,
		WorkStatePerCommit: 500 * kib,
		MaxBuffering:       0,
	})

	// Same as the 2GiB test: chunkSize=1266.
	assert.Equal(t, 1266, s.ChunkSize)
	assert.Equal(t, 1, s.BufferingFactor)
}

// Three-metric adaptive feedback tests (4.3).

// T-1: All three metrics match prediction — no replan.
func TestThreeMetric_AllMatch_NoReplan(t *testing.T) {
	t.Parallel()

	ap := NewAdaptivePlanner(100000, 2048*mib, 500*kib, 400*mib)
	chunks := ap.InitialPlan()
	require.Greater(t, len(chunks), 1)

	predicted := int64(750 * kib)
	chunk := chunks[0]

	newChunks := ap.Replan(ReplanObservation{
		ChunkIndex:          0,
		Chunk:               chunk,
		WorkGrowthPerCommit: predicted,
		TCPayloadPerCommit:  predicted,
		AggGrowthPerCommit:  predicted,
		CurrentChunks:       chunks,
	})

	assert.Len(t, newChunks, len(chunks))
	assert.Equal(t, 0, ap.Stats().ReplanCount)
}

// T-2: Work growth 3x prediction — replan triggered, smaller chunks.
func TestThreeMetric_WorkGrowthHigh_SmallerChunks(t *testing.T) {
	t.Parallel()

	ap := NewAdaptivePlanner(100000, 2048*mib, 500*kib, 400*mib)
	chunks := ap.InitialPlan()
	originalLen := len(chunks)
	predicted := int64(750 * kib)

	newChunks := ap.Replan(ReplanObservation{
		ChunkIndex:          0,
		Chunk:               chunks[0],
		WorkGrowthPerCommit: 3 * predicted,
		TCPayloadPerCommit:  predicted,
		AggGrowthPerCommit:  predicted,
		CurrentChunks:       chunks,
	})

	assert.Greater(t, len(newChunks), originalLen)
	assert.Equal(t, 1, ap.Stats().ReplanCount)
}

// T-3: TC size 3x prediction, work matches — replan triggered.
func TestThreeMetric_TCDiverges_ReplanTriggered(t *testing.T) {
	t.Parallel()

	ap := NewAdaptivePlanner(100000, 2048*mib, 500*kib, 400*mib)
	chunks := ap.InitialPlan()
	originalLen := len(chunks)
	predicted := int64(750 * kib)

	newChunks := ap.Replan(ReplanObservation{
		ChunkIndex:          0,
		Chunk:               chunks[0],
		WorkGrowthPerCommit: predicted,
		TCPayloadPerCommit:  3 * predicted,
		AggGrowthPerCommit:  predicted,
		CurrentChunks:       chunks,
	})

	// TC divergence triggers replan but work EMA drives chunk sizing.
	// Since work growth matches predicted, the chunk sizing uses work EMA
	// which was just initialized to predicted — chunks may not change in count
	// but replan was triggered.
	assert.Equal(t, 1, ap.Stats().ReplanCount)
	// Chunk count may differ slightly due to work EMA being set to observed value.
	_ = originalLen
	_ = newChunks
}

// T-4: Agg growth 3x prediction, work matches — replan triggered.
func TestThreeMetric_AggDiverges_ReplanTriggered(t *testing.T) {
	t.Parallel()

	ap := NewAdaptivePlanner(100000, 2048*mib, 500*kib, 400*mib)
	chunks := ap.InitialPlan()
	predicted := int64(750 * kib)

	newChunks := ap.Replan(ReplanObservation{
		ChunkIndex:          0,
		Chunk:               chunks[0],
		WorkGrowthPerCommit: predicted,
		TCPayloadPerCommit:  predicted,
		AggGrowthPerCommit:  3 * predicted,
		CurrentChunks:       chunks,
	})

	assert.Equal(t, 1, ap.Stats().ReplanCount)
	assert.NotEmpty(t, newChunks)
}

// T-5: Work growth below prediction — replan triggered, larger chunks.
func TestThreeMetric_WorkGrowthLow_LargerChunks(t *testing.T) {
	t.Parallel()

	ap := NewAdaptivePlanner(100000, 2048*mib, 2*mib, 400*mib)
	chunks := ap.InitialPlan()
	originalLen := len(chunks)
	predicted := int64(3 * mib) // 2 MiB * 1.5 effective.

	newChunks := ap.Replan(ReplanObservation{
		ChunkIndex:          0,
		Chunk:               chunks[0],
		WorkGrowthPerCommit: 200 * kib,
		TCPayloadPerCommit:  predicted,
		AggGrowthPerCommit:  predicted,
		CurrentChunks:       chunks,
	})

	assert.Less(t, len(newChunks), originalLen)
	assert.Equal(t, 1, ap.Stats().ReplanCount)
}

// T-6: Mixed: work matches, tc+agg both diverge — replan triggered.
func TestThreeMetric_MixedDivergence_ReplanTriggered(t *testing.T) {
	t.Parallel()

	ap := NewAdaptivePlanner(100000, 2048*mib, 500*kib, 400*mib)
	chunks := ap.InitialPlan()
	predicted := int64(750 * kib)

	newChunks := ap.Replan(ReplanObservation{
		ChunkIndex:          0,
		Chunk:               chunks[0],
		WorkGrowthPerCommit: predicted,
		TCPayloadPerCommit:  3 * predicted,
		AggGrowthPerCommit:  3 * predicted,
		CurrentChunks:       chunks,
	})

	assert.Equal(t, 1, ap.Stats().ReplanCount)
	assert.NotEmpty(t, newChunks)
}

// T-7: All metrics zero → clamped to minimum, no panic.
func TestThreeMetric_AllZero_Clamped(t *testing.T) {
	t.Parallel()

	ap := NewAdaptivePlanner(10000, 2048*mib, 500*kib, 400*mib)
	chunks := ap.InitialPlan()
	require.Greater(t, len(chunks), 1)

	newChunks := ap.Replan(ReplanObservation{
		ChunkIndex:          0,
		Chunk:               chunks[0],
		WorkGrowthPerCommit: 0,
		TCPayloadPerCommit:  0,
		AggGrowthPerCommit:  0,
		CurrentChunks:       chunks,
	})

	assert.NotEmpty(t, newChunks)

	stats := ap.Stats()
	// All EMAs should be clamped to minObservedGrowth (1 KiB).
	assert.InDelta(t, float64(minObservedGrowth), stats.FinalWorkGrowth, 1.0)
	assert.InDelta(t, float64(minObservedGrowth), stats.FinalTCSize, 1.0)
	assert.InDelta(t, float64(minObservedGrowth), stats.FinalAggGrowth, 1.0)
}

// T-8: Stats include per-metric final rates.
func TestThreeMetric_Stats_PerMetricRates(t *testing.T) {
	t.Parallel()

	ap := NewAdaptivePlanner(100000, 2048*mib, 500*kib, 400*mib)
	chunks := ap.InitialPlan()

	// Feed specific values.
	ap.Replan(ReplanObservation{
		ChunkIndex:          0,
		Chunk:               chunks[0],
		WorkGrowthPerCommit: 800 * kib,
		TCPayloadPerCommit:  200 * kib,
		AggGrowthPerCommit:  400 * kib,
		CurrentChunks:       chunks,
	})

	stats := ap.Stats()
	// First observation initializes EMAs directly.
	assert.InDelta(t, float64(800*kib), stats.FinalWorkGrowth, 1.0)
	assert.InDelta(t, float64(200*kib), stats.FinalTCSize, 1.0)
	assert.InDelta(t, float64(400*kib), stats.FinalAggGrowth, 1.0)
	assert.InDelta(t, float64(800*kib), stats.FinalGrowthRate, 1.0)
}

// T-9: Existing tests adapted — covered by the updated tests above.

// T-10 and T-11: Runner integration tests (AggregatorStateSize, TCCountAccumulated)
// are in pkg/framework/runner_test.go.

// assertChunksContiguous verifies that chunks cover [0, totalCommits) without gaps.
func assertChunksContiguous(t *testing.T, chunks []ChunkBounds, totalCommits int) {
	t.Helper()

	require.NotEmpty(t, chunks)
	assert.Equal(t, 0, chunks[0].Start)

	for i := 1; i < len(chunks); i++ {
		assert.Equal(t, chunks[i-1].End, chunks[i].Start, "gap between chunk %d and %d", i-1, i)
	}

	assert.Equal(t, totalCommits, chunks[len(chunks)-1].End)
}
