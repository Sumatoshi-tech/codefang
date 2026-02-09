package streaming

// Size constants.
const (
	kib = 1024
	mib = 1024 * kib
)

// Planner constraints.
const (
	// MinChunkSize is the minimum commits per chunk to amortize hibernation cost.
	MinChunkSize = 200

	// MaxChunkSize is the maximum commits per chunk to bound memory growth.
	// Smaller chunks reduce peak memory by enabling hibernation between chunks.
	// On large repos (kubernetes), 500-commit chunks keep memory under control
	// while amortizing the hibernate/boot overhead.
	MaxChunkSize = 500

	// BaseOverhead is the fixed memory overhead for Go runtime + libgit2 + caches.
	BaseOverhead = 400 * mib

	// AvgStateGrowthPerCommit is the estimated memory growth per commit for analyzer state.
	// On large repos (kubernetes), burndown+couples+shotness accumulate ~500KB/commit.
	AvgStateGrowthPerCommit = 500 * kib
)

// Planner calculates chunk boundaries for streaming execution.
type Planner struct {
	TotalCommits int
	MemoryBudget int64
}

// ChunkBounds represents a chunk of commits to process.
type ChunkBounds struct {
	Start int // Inclusive index.
	End   int // Exclusive index.
}

// Plan returns chunk boundaries as [start, end) index pairs.
func (p *Planner) Plan() []ChunkBounds {
	if p.TotalCommits <= 0 {
		return nil
	}

	chunkSize := p.calculateChunkSize()

	// Single chunk if all commits fit.
	if p.TotalCommits <= chunkSize {
		return []ChunkBounds{{Start: 0, End: p.TotalCommits}}
	}

	// Split into multiple chunks.
	var chunks []ChunkBounds

	for start := 0; start < p.TotalCommits; start += chunkSize {
		end := min(start+chunkSize, p.TotalCommits)
		chunks = append(chunks, ChunkBounds{Start: start, End: end})
	}

	return chunks
}

// calculateChunkSize determines the optimal chunk size based on budget.
func (p *Planner) calculateChunkSize() int {
	if p.MemoryBudget <= 0 {
		return MaxChunkSize
	}

	// Available memory for analyzer state (after overhead).
	available := p.MemoryBudget - BaseOverhead
	if available <= 0 {
		return MinChunkSize
	}

	// Max commits that fit in available memory.
	maxCommits := int(available / AvgStateGrowthPerCommit)

	// Clamp to bounds.
	if maxCommits < MinChunkSize {
		return MinChunkSize
	}

	if maxCommits > MaxChunkSize {
		return MaxChunkSize
	}

	return maxCommits
}
