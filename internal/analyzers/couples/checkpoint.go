package couples

import (
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/internal/checkpoint"
	"github.com/Sumatoshi-tech/codefang/pkg/alg/bloom"
)

// checkpointBasename is the base filename for checkpoint files.
const checkpointBasename = "couples_state"

// checkpointState holds the serializable working state of the couples analyzer.
type checkpointState struct {
	SeenFilesBloom     []byte   `json:"seen_files_bloom"`
	MergesBloom        []byte   `json:"merges_bloom"`
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
	seenData, err := c.seenFiles.MarshalBinary()
	if err != nil {
		seenData = nil
	}

	mergesData, err := c.merges.MarshalBinary()
	if err != nil {
		mergesData = nil
	}

	return &checkpointState{
		SeenFilesBloom:     seenData,
		MergesBloom:        mergesData,
		PeopleNumber:       c.PeopleNumber,
		ReversedPeopleDict: c.reversedPeopleDict,
	}
}

// restoreFromCheckpoint restores analyzer state from a checkpoint.
func (c *HistoryAnalyzer) restoreFromCheckpoint(state *checkpointState) {
	if len(state.SeenFilesBloom) > 0 {
		f := &bloom.Filter{}

		err := f.UnmarshalBinary(state.SeenFilesBloom)
		if err == nil {
			c.seenFiles = f
		} else {
			c.seenFiles = newSeenFilesFilter()
		}
	} else {
		c.seenFiles = newSeenFilesFilter()
	}

	if len(state.MergesBloom) > 0 {
		mt := analyze.NewMergeTracker()

		err := mt.UnmarshalBinary(state.MergesBloom)
		if err == nil {
			c.merges = mt
		} else {
			c.merges = analyze.NewMergeTracker()
		}
	} else {
		c.merges = analyze.NewMergeTracker()
	}

	c.PeopleNumber = state.PeopleNumber
	c.reversedPeopleDict = state.ReversedPeopleDict
}

// checkpointBaseOverhead is the minimum checkpoint size in bytes.
const checkpointBaseOverhead = 100

// Checkpoint size estimation constants.
const bytesPerPerson = 50

// CheckpointSize returns an estimated size of the checkpoint in bytes.
func (c *HistoryAnalyzer) CheckpointSize() int64 {
	size := int64(checkpointBaseOverhead)

	// Bloom filters: 24-byte header + 8 bytes per uint64 word each.
	if c.seenFiles != nil {
		size += 24 + int64(c.seenFiles.BitCount()/64+1)*8 //nolint:mnd // bloom filter binary layout constants.
	}

	// Merge tracker Bloom filter (fixed size based on mergeTrackerExpected).
	if c.merges != nil {
		mergesData, _ := c.merges.MarshalBinary() //nolint:errcheck // best-effort size estimation.
		size += int64(len(mergesData))
	}

	size += int64(len(c.reversedPeopleDict)) * bytesPerPerson

	return size
}
