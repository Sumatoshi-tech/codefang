package streaming

import (
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestPlanner_SmallRepo_SingleChunk(t *testing.T) {
	p := Planner{
		TotalCommits: 1000,
		MemoryBudget: 512 * mib,
	}
	chunks := p.Plan()
	require.Len(t, chunks, 1)
	assert.Equal(t, 0, chunks[0].Start)
	assert.Equal(t, 1000, chunks[0].End)
}

func TestPlanner_LargeRepo_MultipleChunks(t *testing.T) {
	// 100k commits with 128MiB budget
	// Available for state = 128MiB - 50MiB overhead = 78MiB
	// At 2KiB per commit, can fit ~40k commits per chunk
	// So 100k commits should yield ~3 chunks
	p := Planner{
		TotalCommits: 100000,
		MemoryBudget: 128 * mib,
	}
	chunks := p.Plan()
	require.Greater(t, len(chunks), 1)

	// Verify chunks are contiguous
	assert.Equal(t, 0, chunks[0].Start)
	for i := 1; i < len(chunks); i++ {
		assert.Equal(t, chunks[i-1].End, chunks[i].Start)
	}
	assert.Equal(t, 100000, chunks[len(chunks)-1].End)
}

func TestPlanner_ZeroCommits_Empty(t *testing.T) {
	p := Planner{
		TotalCommits: 0,
		MemoryBudget: 512 * mib,
	}
	chunks := p.Plan()
	assert.Empty(t, chunks)
}

func TestPlanner_ChunkSizeRespectsBounds(t *testing.T) {
	// Very tight budget should use MinChunkSize
	p := Planner{
		TotalCommits: 100000,
		MemoryBudget: 60 * mib, // Only 10MiB available after overhead
	}
	chunks := p.Plan()
	require.NotEmpty(t, chunks)

	// Verify all chunks respect min/max bounds
	for _, chunk := range chunks {
		size := chunk.End - chunk.Start
		// Last chunk may be smaller, but others should be at least MinChunkSize
		if chunk.End < p.TotalCommits {
			assert.GreaterOrEqual(t, size, MinChunkSize)
		}
		assert.LessOrEqual(t, size, MaxChunkSize)
	}
}

func TestPlanner_NoBudget_UsesMaxChunkSize(t *testing.T) {
	p := Planner{
		TotalCommits: 50000,
		MemoryBudget: 0, // No budget constraint
	}
	chunks := p.Plan()
	require.NotEmpty(t, chunks)

	// Without budget, should use MaxChunkSize (5000)
	// 50k commits / 5k max = 10 chunks
	assert.Len(t, chunks, 10)
}
