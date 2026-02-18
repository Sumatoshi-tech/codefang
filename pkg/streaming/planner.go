package streaming

// Size constants.
const (
	kib = 1024
	mib = 1024 * kib
)

// Planner constraints.
const (
	// MinChunkSize is the minimum commits per chunk to amortize hibernation cost.
	MinChunkSize = 50

	// MaxChunkSize is the safety cap on commits per chunk. The primary constraint
	// is the memory budget divided by per-analyzer growth rate, not this cap.
	MaxChunkSize = 3000

	// BaseOverhead is the fixed memory overhead for Go runtime + libgit2 + caches.
	BaseOverhead = 400 * mib

	// safetyMarginPercent is added to the aggregate growth rate to account for
	// transient pipeline allocations (tree diffs, blobs in flight, GC headroom)
	// that scale with chunk size but aren't captured by analyzer-declared growth.
	safetyMarginPercent = 50

	// percentDivisor converts safetyMarginPercent to a fraction.
	percentDivisor = 100

	// DefaultStateGrowthPerCommit is the conservative fallback for analyzers
	// that don't implement MemoryWeighter. Matches burndown, the heaviest analyzer.
	DefaultStateGrowthPerCommit = 500 * kib
)

// MemoryWeighter is an optional interface that analyzers can implement to declare
// their approximate per-commit state growth in bytes. The planner sums these
// across selected analyzers to compute an optimal chunk size.
type MemoryWeighter interface {
	StateGrowthPerCommit() int64
}

// GrowthOrDefault returns the analyzer's declared per-commit growth if it
// implements MemoryWeighter, otherwise DefaultStateGrowthPerCommit.
func GrowthOrDefault(a any) int64 {
	if mw, ok := a.(MemoryWeighter); ok {
		return mw.StateGrowthPerCommit()
	}

	return DefaultStateGrowthPerCommit
}

// Planner calculates chunk boundaries for streaming execution.
type Planner struct {
	TotalCommits int
	MemoryBudget int64

	// AggregateGrowthPerCommit is the summed per-commit state growth across all
	// selected leaf analyzers. When zero, DefaultStateGrowthPerCommit is used.
	AggregateGrowthPerCommit int64

	// PipelineOverhead is the estimated memory consumed by caches, workers, and
	// buffers (everything except analyzer state). When positive, it replaces
	// BaseOverhead for more accurate chunk sizing. The budget solver provides
	// this value via EstimateMemoryUsage.
	PipelineOverhead int64
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

// calculateChunkSize determines the optimal chunk size based on budget and
// the aggregate per-commit growth rate of selected analyzers.
func (p *Planner) calculateChunkSize() int {
	if p.MemoryBudget <= 0 {
		return MaxChunkSize
	}

	// Available memory for analyzer state (after overhead).
	overhead := int64(BaseOverhead)
	if p.PipelineOverhead > 0 {
		overhead = p.PipelineOverhead
	}

	available := p.MemoryBudget - overhead
	if available <= 0 {
		return MinChunkSize
	}

	growth := p.AggregateGrowthPerCommit
	if growth <= 0 {
		growth = DefaultStateGrowthPerCommit
	}

	// Add safety margin for transient pipeline allocations.
	growth += growth * safetyMarginPercent / percentDivisor

	// Max commits that fit in available memory.
	maxCommits := int(available / growth)

	return max(min(maxCommits, MaxChunkSize), MinChunkSize)
}
