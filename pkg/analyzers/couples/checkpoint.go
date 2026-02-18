package couples

import (
	"github.com/Sumatoshi-tech/codefang/pkg/checkpoint"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

// checkpointBasename is the base filename for checkpoint files.
const checkpointBasename = "couples_state"

// checkpointState holds the serializable state of the couples analyzer.
type checkpointState struct {
	Files              map[string]map[string]int `json:"files"`
	People             []map[string]int          `json:"people"`
	PeopleCommits      []int                     `json:"people_commits"`
	Merges             []string                  `json:"merges"`
	Renames            []rename                  `json:"renames"`
	PeopleNumber       int                       `json:"people_number"`
	ReversedPeopleDict []string                  `json:"reversed_people_dict"`
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
	state := &checkpointState{
		Files:              c.files,
		People:             c.people,
		PeopleCommits:      c.peopleCommits,
		Merges:             make([]string, 0, len(c.merges)),
		PeopleNumber:       c.PeopleNumber,
		ReversedPeopleDict: c.reversedPeopleDict,
	}

	// Convert renames pointer to slice.
	if c.renames != nil {
		state.Renames = *c.renames
	}

	// Convert merge hashes to strings.
	for hash := range c.merges {
		state.Merges = append(state.Merges, hash.String())
	}

	return state
}

// restoreFromCheckpoint restores analyzer state from a checkpoint.
func (c *HistoryAnalyzer) restoreFromCheckpoint(state *checkpointState) {
	c.files = state.Files
	c.people = state.People
	c.peopleCommits = state.PeopleCommits
	c.PeopleNumber = state.PeopleNumber
	c.reversedPeopleDict = state.ReversedPeopleDict

	// Convert renames slice to pointer.
	c.renames = &state.Renames

	// Convert merge hash strings back to hashes.
	c.merges = make(map[gitlib.Hash]bool, len(state.Merges))
	for _, hashStr := range state.Merges {
		c.merges[gitlib.NewHash(hashStr)] = true
	}
}

// Checkpoint size estimation constants.
const (
	baseOverheadBytes    = 100
	bytesPerFilePair     = 80
	bytesPerPersonFile   = 60
	bytesPerMerge        = 44
	bytesPerRename       = 100
	bytesPerPerson       = 50
	bytesPerPeopleCommit = 8
)

// CheckpointSize returns an estimated size of the checkpoint in bytes.
func (c *HistoryAnalyzer) CheckpointSize() int64 {
	size := int64(baseOverheadBytes)

	// Count file pairs.
	for _, couplings := range c.files {
		size += int64(len(couplings) * bytesPerFilePair)
	}

	// Count person-file entries.
	for _, files := range c.people {
		size += int64(len(files) * bytesPerPersonFile)
	}

	// Count merge entries.
	size += int64(len(c.merges) * bytesPerMerge)

	// Count rename entries.
	if c.renames != nil {
		size += int64(len(*c.renames) * bytesPerRename)
	}

	// Count people entries.
	size += int64(len(c.reversedPeopleDict) * bytesPerPerson)

	// Count peopleCommits entries.
	size += int64(len(c.peopleCommits) * bytesPerPeopleCommit)

	return size
}
