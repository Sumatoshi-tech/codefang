package couples

import (
	"github.com/Sumatoshi-tech/codefang/pkg/checkpoint"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

// checkpointBasename is the base filename for checkpoint files.
const checkpointBasename = "couples_state"

// checkpointState holds the serializable working state of the couples analyzer.
type checkpointState struct {
	SeenFiles          []string `json:"seen_files"`
	Merges             []string `json:"merges"`
	PeopleNumber       int      `json:"people_number"`
	ReversedPeopleDict []string `json:"reversed_people_dict"`
}

// newPersister creates a checkpoint persister for couples analyzer.
func newPersister() *checkpoint.Persister[checkpointState] {
	return checkpoint.NewPersister[checkpointState](
		checkpointBasename,
		checkpoint.NewJSONCodec(),
	)
}

// SaveCheckpoint writes the analyzer state to the given directory.
func (c *HistoryAnalyzer) SaveCheckpoint(dir string) error {
	return newPersister().Save(dir, c.buildCheckpointState)
}

// LoadCheckpoint restores the analyzer state from the given directory.
func (c *HistoryAnalyzer) LoadCheckpoint(dir string) error {
	return newPersister().Load(dir, c.restoreFromCheckpoint)
}

// buildCheckpointState creates a serializable snapshot of the analyzer state.
func (c *HistoryAnalyzer) buildCheckpointState() *checkpointState {
	seenFiles := make([]string, 0, len(c.seenFiles))
	for f := range c.seenFiles {
		seenFiles = append(seenFiles, f)
	}

	merges := make([]string, 0, len(c.merges))
	for hash := range c.merges {
		merges = append(merges, hash.String())
	}

	return &checkpointState{
		SeenFiles:          seenFiles,
		Merges:             merges,
		PeopleNumber:       c.PeopleNumber,
		ReversedPeopleDict: c.reversedPeopleDict,
	}
}

// restoreFromCheckpoint restores analyzer state from a checkpoint.
func (c *HistoryAnalyzer) restoreFromCheckpoint(state *checkpointState) {
	c.seenFiles = make(map[string]bool, len(state.SeenFiles))
	for _, f := range state.SeenFiles {
		c.seenFiles[f] = true
	}

	c.merges = make(map[gitlib.Hash]bool, len(state.Merges))
	for _, hashStr := range state.Merges {
		c.merges[gitlib.NewHash(hashStr)] = true
	}

	c.PeopleNumber = state.PeopleNumber
	c.reversedPeopleDict = state.ReversedPeopleDict
}

// checkpointBaseOverhead is the minimum checkpoint size in bytes.
const checkpointBaseOverhead = 100

// Checkpoint size estimation constants.
const (
	bytesPerSeenFile = 60
	bytesPerMerge    = 44
	bytesPerPerson   = 50
)

// CheckpointSize returns an estimated size of the checkpoint in bytes.
func (c *HistoryAnalyzer) CheckpointSize() int64 {
	size := int64(checkpointBaseOverhead)
	size += int64(len(c.seenFiles)) * bytesPerSeenFile
	size += int64(len(c.merges)) * bytesPerMerge
	size += int64(len(c.reversedPeopleDict)) * bytesPerPerson

	return size
}
