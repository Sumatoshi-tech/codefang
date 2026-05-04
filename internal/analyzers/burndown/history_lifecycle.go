package burndown

import (
	"fmt"
	"log"
	"os"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/common"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/plumbing"
)

// Fork creates a copy of the analyzer for parallel processing.
func (b *HistoryAnalyzer) Fork(n int) []analyze.HistoryAnalyzer {
	res := make([]analyze.HistoryAnalyzer, n)

	for i := range n {
		res[i] = b.newForkedClone()
	}

	return res
}

// newForkedClone creates an independent HistoryAnalyzer clone for parallel chunk processing.
func (b *HistoryAnalyzer) newForkedClone() *HistoryAnalyzer {
	clone := &HistoryAnalyzer{
		pathInterner: b.pathInterner,
		repository:   b.repository,
		IdentityMixin: common.IdentityMixin{
			Identity:           &plumbing.IdentityDetector{},
			ReversedPeopleDict: b.GetReversedPeopleDict(),
		},
		TreeDiff:             &plumbing.TreeDiffAnalyzer{},
		Ticks:                &plumbing.TicksSinceStart{},
		BlobCache:            &plumbing.BlobCacheAnalyzer{},
		FileDiff:             &plumbing.FileDiffAnalyzer{},
		HibernationDirectory: b.HibernationDirectory,
		HibernationThreshold: b.HibernationThreshold,
		Granularity:          b.Granularity,
		PeopleNumber:         b.PeopleNumber,
		TickSize:             b.TickSize,
		Goroutines:           b.Goroutines,
		Sampling:             b.Sampling,
		Debug:                b.Debug,
		TrackFiles:           b.TrackFiles,
		HibernationToDisk:    b.HibernationToDisk,
	}

	clone.initFreshShards(b.Goroutines)
	clone.renames = map[string]string{}
	clone.renamesReverse = map[string]map[string]bool{}

	return clone
}

// initFreshShards allocates clean shard state for a new or forked analyzer.
func (b *HistoryAnalyzer) initFreshShards(count int) {
	b.shards = make([]*Shard, count)

	for i := range count {
		b.shards[i] = &Shard{
			mergedByID:    map[PathID]bool{},
			deletionsByID: map[PathID]bool{},
		}
	}
}

// Merge combines results from forked analyzer branches.
func (b *HistoryAnalyzer) Merge(branches []analyze.HistoryAnalyzer) {
	for _, branch := range branches {
		other, ok := branch.(*HistoryAnalyzer)
		if !ok {
			continue
		}

		b.mergeShards(other)
		b.mergeRenameTracking(other)
		b.mergeTicks(other)
	}
}

// mergeShards is a no-op after TC migration — delta buffers are per-commit.
func (b *HistoryAnalyzer) mergeShards(_ *HistoryAnalyzer) {}

// mergeRenameTracking merges rename tracking from another analyzer.
func (b *HistoryAnalyzer) mergeRenameTracking(other *HistoryAnalyzer) {
	b.GlobalMu.Lock()
	defer b.GlobalMu.Unlock()

	for from, to := range other.renames {
		if to == "" {
			continue
		}

		b.renames[from] = to

		if b.renamesReverse[to] == nil {
			b.renamesReverse[to] = map[string]bool{}
		}

		b.renamesReverse[to][from] = true
	}
}

// mergeTicks updates tick tracking from another analyzer.
func (b *HistoryAnalyzer) mergeTicks(other *HistoryAnalyzer) {
	if other.tick > b.tick {
		b.tick = other.tick
	}

	if other.previousTick > b.previousTick {
		b.previousTick = other.previousTick
	}

	if !other.lastCommitTime.IsZero() && other.lastCommitTime.After(b.lastCommitTime) {
		b.lastCommitTime = other.lastCommitTime
	}
}

// Hibernate releases resources between processing phases.
func (b *HistoryAnalyzer) Hibernate() error {
	b.logChunkMismatchSummary()

	err := b.ensureSpillDir()
	if err != nil {
		return fmt.Errorf("burndown spill dir: %w", err)
	}

	for i, shard := range b.shards {
		err = b.hibernateShard(shard, i)
		if err != nil {
			return err
		}
	}

	b.GlobalMu.Lock()
	b.compactRenameMaps()
	b.GlobalMu.Unlock()

	return nil
}

// logChunkMismatchSummary emits a single line summarizing src-mismatch
// resets recorded since the last chunk boundary, then re-baselines the
// counter for the next chunk. Silent when no mismatches happened.
func (b *HistoryAnalyzer) logChunkMismatchSummary() {
	delta := b.mismatch.chunkDelta()
	if delta == 0 {
		b.mismatch.resetChunkBaseline()

		return
	}

	stats := b.mismatch.snapshot()
	log.Printf("burndown: chunk src-mismatch summary chunk_resets=%d cumulative_resets=%d cumulative_force_removes=%d",
		delta, stats.Resets, stats.ForceRemoves)

	b.mismatch.resetChunkBaseline()
}

// hibernateShard shrinks treap pools, spills to disk, and resets tracking maps.
func (b *HistoryAnalyzer) hibernateShard(shard *Shard, idx int) error {
	shard.mu.Lock()
	defer shard.mu.Unlock()

	b.shrinkShardPools(shard)

	if b.spillDir != "" {
		spillErr := spillShardFiles(shard, &b.shardSpills[idx], b.spillDir, idx)
		if spillErr != nil {
			return fmt.Errorf("burndown spill shard %d files: %w", idx, spillErr)
		}
	}

	shard.mergedByID = make(map[PathID]bool)
	shard.deletionsByID = make(map[PathID]bool)

	return nil
}

// shrinkShardPools releases excess free-list memory from file treap pools.
func (b *HistoryAnalyzer) shrinkShardPools(shard *Shard) {
	for _, id := range shard.activeIDs {
		if int(id) < len(shard.filesByID) {
			if file := shard.filesByID[id]; file != nil {
				file.ShrinkPool(0)
			}
		}
	}
}

// compactRenameMaps replaces renames/renamesReverse with fresh maps
// containing only live (non-empty) entries. Must be called with GlobalMu held.
func (b *HistoryAnalyzer) compactRenameMaps() {
	if len(b.renames) == 0 {
		return
	}

	fresh := make(map[string]string, len(b.renames)/renameCapDivisor)

	for from, to := range b.renames {
		if to != "" {
			fresh[from] = to
		}
	}

	b.renames = fresh

	freshReverse := make(map[string]map[string]bool, len(b.renamesReverse))

	for to, froms := range b.renamesReverse {
		if len(froms) > 0 {
			freshReverse[to] = froms
		}
	}

	b.renamesReverse = freshReverse
}

// ensureSpillDir creates the parent temp directory for shard history spills.
func (b *HistoryAnalyzer) ensureSpillDir() error {
	if b.spillDir != "" {
		return nil
	}

	dir, err := os.MkdirTemp(b.HibernationDirectory, "codefang-burndown-spill-*")
	if err != nil {
		return fmt.Errorf("create burndown spill dir: %w", err)
	}

	b.spillDir = dir

	return nil
}

// Boot restores spilled file treaps and re-attaches updaters for the next chunk.
func (b *HistoryAnalyzer) Boot() error {
	for i, shard := range b.shards {
		err := b.bootShard(shard, i)
		if err != nil {
			return err
		}
	}

	b.resetDeltaBuffers()

	return nil
}

// bootShard restores spilled files, re-attaches updaters, and ensures tracking maps.
func (b *HistoryAnalyzer) bootShard(shard *Shard, idx int) error {
	shard.mu.Lock()
	defer shard.mu.Unlock()

	err := b.restoreSpilledShard(shard, idx)
	if err != nil {
		return err
	}

	b.ensureShardMaps(shard)

	return nil
}

// restoreSpilledShard loads spilled files and re-attaches updaters.
func (b *HistoryAnalyzer) restoreSpilledShard(shard *Shard, idx int) error {
	if b.spillDir == "" || idx >= len(b.shardSpills) || b.shardSpills[idx].fileSpillN == 0 {
		return nil
	}

	err := loadSpilledFiles(shard, &b.shardSpills[idx])
	if err != nil {
		return fmt.Errorf("restore shard %d files: %w", idx, err)
	}

	b.reattachUpdaters(shard)

	return nil
}

// reattachUpdaters re-binds updater closures to restored file treaps.
func (b *HistoryAnalyzer) reattachUpdaters(shard *Shard) {
	for _, id := range shard.activeIDs {
		if int(id) >= len(shard.filesByID) {
			continue
		}

		if file := shard.filesByID[id]; file != nil {
			file.ReplaceUpdaters(b.createUpdaters(shard, id))
		}
	}
}

// ensureShardMaps initializes nil tracking maps for a shard.
func (b *HistoryAnalyzer) ensureShardMaps(shard *Shard) {
	if shard.mergedByID == nil {
		shard.mergedByID = make(map[PathID]bool)
	}

	if shard.deletionsByID == nil {
		shard.deletionsByID = make(map[PathID]bool)
	}
}

// CleanupSpills removes all shard spill temp files. Safe to call multiple times.
func (b *HistoryAnalyzer) CleanupSpills() {
	for i := range b.shardSpills {
		cleanupShardSpills(&b.shardSpills[i])
	}

	if b.spillDir != "" {
		os.RemoveAll(b.spillDir)
		b.spillDir = ""
	}
}
