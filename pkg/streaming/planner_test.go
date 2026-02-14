package streaming

import (
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestPlanner_SmallRepo_SingleChunk(t *testing.T) {
	t.Parallel()

	// 400 commits fits in a single chunk (MaxChunkSize=500).
	p := Planner{
		TotalCommits: 400,
		MemoryBudget: 2000 * mib,
	}
	chunks := p.Plan()
	require.Len(t, chunks, 1)
	assert.Equal(t, 0, chunks[0].Start)
	assert.Equal(t, 400, chunks[0].End)
}

func TestPlanner_LargeRepo_MultipleChunks(t *testing.T) {
	t.Parallel()

	// 100k commits with 2GiB budget.
	// Available for state = 2048MiB - 400MiB overhead = 1648MiB
	// At 500KiB per commit, can fit ~3296 commits per chunk → clamped to MaxChunkSize=500
	// So 100k commits / 500 = 200 chunks.
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
	// At 500KiB/commit, that's 20 commits → clamped to MinChunkSize=200.
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
		TotalCommits: 50000,
		MemoryBudget: 0, // No budget constraint.
	}
	chunks := p.Plan()
	require.NotEmpty(t, chunks)

	// Without budget, should use MaxChunkSize (500)
	// 50k commits / 500 max = 100 chunks.
	assert.Len(t, chunks, 100)
}
