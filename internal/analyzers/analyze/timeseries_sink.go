package analyze

import (
	"fmt"
	"io"
	"sort"
	"sync"

	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

// TimeSeriesChunkFlusher drains per-commit data from aggregators after each
// chunk and writes NDJSON lines. This keeps memory bounded to O(chunk_size)
// instead of O(total_commits) for large repositories.
type TimeSeriesChunkFlusher struct {
	mu     sync.Mutex
	writer io.Writer
	leaves []HistoryAnalyzer // in original registration order (matches aggregators).
}

// NewTimeSeriesChunkFlusher creates a flusher that writes NDJSON lines to writer.
// leaves must be in the same order as the aggregators returned by runner.LeafAggregators().
func NewTimeSeriesChunkFlusher(writer io.Writer, leaves []HistoryAnalyzer) *TimeSeriesChunkFlusher {
	return &TimeSeriesChunkFlusher{
		writer: writer,
		leaves: leaves,
	}
}

// Flush drains per-commit data from all aggregators, merges with commit
// metadata, and writes NDJSON lines. Returns the number of commits flushed.
// aggregators must be in the same order as the leaves passed to the constructor.
func (f *TimeSeriesChunkFlusher) Flush(
	aggregators []Aggregator,
	commitMeta map[string]CommitMeta,
) (int, error) {
	f.mu.Lock()
	defer f.mu.Unlock()

	// Collect per-analyzer commit data and commitsByTick from all drainers.
	var active []AnalyzerData

	var allCommitsByTick map[int][]gitlib.Hash

	for i, leaf := range f.leaves {
		if i >= len(aggregators) || aggregators[i] == nil {
			continue
		}

		drainer, ok := aggregators[i].(CommitStatsDrainer)
		if !ok {
			continue
		}

		commitData, commitsByTick := drainer.DrainCommitStats()
		if len(commitData) == 0 {
			continue
		}

		active = append(active, AnalyzerData{
			Flag: leaf.Flag(),
			Data: commitData,
		})

		// Take commitsByTick from the first aggregator that provides it.
		// All custom aggregators see the same commits, so their commitsByTick
		// are identical — using the first one avoids duplicate entries.
		if allCommitsByTick == nil && len(commitsByTick) > 0 {
			allCommitsByTick = commitsByTick
		}
	}

	if len(active) == 0 {
		return 0, nil
	}

	// Sort active by flag for deterministic output ordering.
	sort.Slice(active, func(i, j int) bool {
		return active[i].Flag < active[j].Flag
	})

	// Build ordered commit meta from drained commitsByTick + runner's commitMeta.
	meta := assembleOrderedCommitMetaFromDrain(allCommitsByTick, commitMeta)

	ts := BuildMergedTimeSeriesDirect(active, meta, 0)

	err := WriteTimeSeriesNDJSON(ts, f.writer)
	if err != nil {
		return 0, fmt.Errorf("flush timeseries chunk: %w", err)
	}

	return len(ts.Commits), nil
}

// assembleOrderedCommitMetaFromDrain builds an ordered CommitMeta slice from
// drained commitsByTick and the runner's accumulated commit metadata map.
// When commitsByTick is nil (only GenericAggregator-based analyzers ran),
// falls back to reconstructing order from commitMetaMap.
func assembleOrderedCommitMetaFromDrain(
	commitsByTick map[int][]gitlib.Hash,
	commitMetaMap map[string]CommitMeta,
) []CommitMeta {
	if len(commitsByTick) > 0 {
		return assembleOrderedCommitMeta(commitsByTick, commitMetaMap)
	}

	// Fallback: reconstruct commitsByTick from commitMetaMap.
	if len(commitMetaMap) == 0 {
		return nil
	}

	reconstructed := make(map[int][]gitlib.Hash)
	for _, cm := range commitMetaMap {
		reconstructed[cm.Tick] = append(reconstructed[cm.Tick], gitlib.NewHash(cm.Hash))
	}

	return assembleOrderedCommitMeta(reconstructed, commitMetaMap)
}
