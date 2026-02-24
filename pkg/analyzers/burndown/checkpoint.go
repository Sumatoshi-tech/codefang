package burndown

import (
	"maps"

	"github.com/Sumatoshi-tech/codefang/pkg/checkpoint"
)

// Checkpoint size estimation constants.
const (
	estimatedBytesPerPath    = 100
	estimatedBytesPerHistory = 100
	estimatedBytesPerActive  = 50
)

const checkpointBasename = "burndown_state"

// newPersister creates a checkpoint persister for burndown analyzer.
func newPersister() *checkpoint.Persister[checkpointState] {
	return checkpoint.NewPersister[checkpointState](
		checkpointBasename,
		checkpoint.NewGobCodec(),
	)
}

// checkpointState holds the serializable state of the burndown analyzer.
type checkpointState struct {
	// PathInterner state.
	PathInternerIDs []pathEntry
	PathInternerRev []string

	// Global state.
	ReversedPeopleDict []string
	Tick               int
	PreviousTick       int

	// Rename tracking.
	Renames        map[string]string
	RenamesReverse map[string]map[string]bool

	// Shard state.
	Shards []shardState
}

// pathEntry represents a path→ID mapping.
type pathEntry struct {
	Path string
	ID   PathID
}

// shardState holds serializable state for a single shard.
type shardState struct {
	FileHistoriesByID []sparseHistory
	ActiveIDs         []PathID
	MergedByID        map[PathID]bool
	DeletionsByID     map[PathID]bool
}

// SaveCheckpoint writes the analyzer state to the given directory.
func (b *HistoryAnalyzer) SaveCheckpoint(dir string) error {
	return newPersister().Save(dir, b.buildCheckpointState)
}

// LoadCheckpoint restores the analyzer state from the given directory.
func (b *HistoryAnalyzer) LoadCheckpoint(dir string) error {
	return newPersister().Load(dir, b.restoreFromCheckpoint)
}

// CheckpointSize returns an estimated size of the checkpoint in bytes.
func (b *HistoryAnalyzer) CheckpointSize() int64 {
	// Rough estimate: pathInterner entries + active file state.
	size := int64(0)
	if b.pathInterner != nil {
		size += int64(len(b.pathInterner.rev) * estimatedBytesPerPath)
	}

	for _, sh := range b.shards {
		size += int64(len(sh.activeIDs) * estimatedBytesPerActive)
	}

	return size
}

// buildCheckpointState creates a serializable snapshot of the analyzer state.
func (b *HistoryAnalyzer) buildCheckpointState() *checkpointState {
	state := &checkpointState{
		ReversedPeopleDict: append([]string{}, b.reversedPeopleDict...),
		Tick:               b.tick,
		PreviousTick:       b.previousTick,
		Renames:            cloneStringMap(b.renames),
		RenamesReverse:     cloneRenamesReverse(b.renamesReverse),
	}

	// Save path interner state.
	if b.pathInterner != nil {
		state.PathInternerRev = append([]string{}, b.pathInterner.rev...)
		state.PathInternerIDs = make([]pathEntry, 0, len(b.pathInterner.ids))

		for path, id := range b.pathInterner.ids {
			state.PathInternerIDs = append(state.PathInternerIDs, pathEntry{Path: path, ID: id})
		}
	}

	// Save shard state (file treaps are working state, history maps are in aggregator).
	state.Shards = make([]shardState, len(b.shards))

	for i, shard := range b.shards {
		shard.mu.Lock()
		state.Shards[i] = shardState{
			FileHistoriesByID: clonePeopleHistories(shard.fileHistoriesByID),
			ActiveIDs:         append([]PathID{}, shard.activeIDs...),
			MergedByID:        clonePathIDMap(shard.mergedByID),
			DeletionsByID:     clonePathIDMap(shard.deletionsByID),
		}
		shard.mu.Unlock()
	}

	return state
}

// restoreFromCheckpoint restores analyzer state from a checkpoint.
func (b *HistoryAnalyzer) restoreFromCheckpoint(state *checkpointState) {
	b.reversedPeopleDict = state.ReversedPeopleDict
	b.tick = state.Tick
	b.previousTick = state.PreviousTick
	b.renames = state.Renames
	b.renamesReverse = state.RenamesReverse

	// Restore path interner.
	if b.pathInterner == nil {
		b.pathInterner = NewPathInterner()
	}

	b.pathInterner.rev = state.PathInternerRev
	b.pathInterner.ids = make(map[string]PathID, len(state.PathInternerIDs))

	for _, entry := range state.PathInternerIDs {
		b.pathInterner.ids[entry.Path] = entry.ID
	}

	// Restore shards (working state only — history maps are in the aggregator).
	for i, ss := range state.Shards {
		if i >= len(b.shards) {
			break
		}

		shard := b.shards[i]
		shard.mu.Lock()
		shard.fileHistoriesByID = ss.FileHistoriesByID
		shard.activeIDs = ss.ActiveIDs
		shard.mergedByID = ss.MergedByID
		shard.deletionsByID = ss.DeletionsByID
		shard.mu.Unlock()
	}
}

// Helper functions for deep cloning.

func cloneSparseHistory(history sparseHistory) sparseHistory {
	if history == nil {
		return nil
	}

	clone := make(sparseHistory, len(history))

	for tick, inner := range history {
		clone[tick] = make(map[int]int64, len(inner))
		maps.Copy(clone[tick], inner)
	}

	return clone
}

func clonePeopleHistories(histories []sparseHistory) []sparseHistory {
	if histories == nil {
		return nil
	}

	clone := make([]sparseHistory, len(histories))

	for i, hist := range histories {
		clone[i] = cloneSparseHistory(hist)
	}

	return clone
}

func cloneStringMap(strMap map[string]string) map[string]string {
	if strMap == nil {
		return nil
	}

	clone := make(map[string]string, len(strMap))
	maps.Copy(clone, strMap)

	return clone
}

func cloneRenamesReverse(renamesRev map[string]map[string]bool) map[string]map[string]bool {
	if renamesRev == nil {
		return nil
	}

	clone := make(map[string]map[string]bool, len(renamesRev))

	for k, v := range renamesRev {
		inner := make(map[string]bool, len(v))
		maps.Copy(inner, v)
		clone[k] = inner
	}

	return clone
}

func clonePathIDMap(pathIDMap map[PathID]bool) map[PathID]bool {
	if pathIDMap == nil {
		return nil
	}

	clone := make(map[PathID]bool, len(pathIDMap))
	maps.Copy(clone, pathIDMap)

	return clone
}
