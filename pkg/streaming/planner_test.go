package streaming

import (
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

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
