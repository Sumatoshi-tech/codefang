package devs

import (
	"github.com/Sumatoshi-tech/codefang/pkg/checkpoint"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

// checkpointBasename is the base filename for checkpoint files (used by tests).
const checkpointBasename = "devs_state"

// Checkpoint size estimation constants.
const (
	baseOverheadBytes   = 100
	bytesPerCommitEntry = 150
	bytesPerMerge       = 44
	bytesPerPerson      = 50
)

// newPersister creates a checkpoint persister for devs analyzer.
func newPersister() *checkpoint.Persister[checkpointState] {
	return checkpoint.NewPersister[checkpointState](
		checkpointBasename,
		checkpoint.NewJSONCodec(),
	)
}

// SaveCheckpoint writes the analyzer state to the given directory.
func (d *HistoryAnalyzer) SaveCheckpoint(dir string) error {
	return newPersister().Save(dir, d.buildCheckpointState)
}

// LoadCheckpoint restores the analyzer state from the given directory.
func (d *HistoryAnalyzer) LoadCheckpoint(dir string) error {
	return newPersister().Load(dir, d.restoreFromCheckpoint)
}

// checkpointState holds the serializable state of the devs analyzer.
type checkpointState struct {
	CommitDevData map[string]*CommitDevData `json:"commit_dev_data"`
	Merges        []string                  `json:"merges"`
}

// buildCheckpointState creates a serializable snapshot of the analyzer state.
func (d *HistoryAnalyzer) buildCheckpointState() *checkpointState {
	state := &checkpointState{
		CommitDevData: d.commitDevData,
		Merges:        make([]string, 0, len(d.merges)),
	}

	for hash := range d.merges {
		state.Merges = append(state.Merges, hash.String())
	}

	return state
}

// restoreFromCheckpoint restores analyzer state from a checkpoint.
func (d *HistoryAnalyzer) restoreFromCheckpoint(state *checkpointState) {
	d.commitDevData = state.CommitDevData
	if d.commitDevData == nil {
		d.commitDevData = make(map[string]*CommitDevData)
	}

	d.merges = make(map[gitlib.Hash]bool, len(state.Merges))

	for _, hashStr := range state.Merges {
		d.merges[gitlib.NewHash(hashStr)] = true
	}
}

// CheckpointSize returns an estimated size of the checkpoint in bytes.
func (d *HistoryAnalyzer) CheckpointSize() int64 {
	size := int64(baseOverheadBytes)

	// Count commit entries (~100 bytes each: hash + stats + author + languages).
	size += int64(len(d.commitDevData) * bytesPerCommitEntry)

	// Count merge entries.
	size += int64(len(d.merges) * bytesPerMerge)

	// Count people entries.
	size += int64(len(d.reversedPeopleDict) * bytesPerPerson)

	return size
}
