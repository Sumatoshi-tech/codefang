package typos

import (
	"encoding/hex"

	"github.com/Sumatoshi-tech/codefang/pkg/checkpoint"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

// checkpointBasename is the base filename for checkpoint files.
const checkpointBasename = "typos_state"

// typoCheckpoint is the serializable form of a Typo.
type typoCheckpoint struct {
	Wrong   string `json:"wrong"`
	Correct string `json:"correct"`
	File    string `json:"file"`
	Commit  string `json:"commit"` // Hash as hex string.
	Line    int    `json:"line"`
}

// checkpointState holds the serializable state of the typos analyzer.
type checkpointState struct {
	Typos []typoCheckpoint `json:"typos"`
}

// newPersister creates a checkpoint persister for typos analyzer.
func newPersister() *checkpoint.Persister[checkpointState] {
	return checkpoint.NewPersister[checkpointState](
		checkpointBasename,
		checkpoint.NewJSONCodec(),
	)
}

// SaveCheckpoint writes the analyzer state to the given directory.
func (t *HistoryAnalyzer) SaveCheckpoint(dir string) error {
	return newPersister().Save(dir, t.buildCheckpointState)
}

// LoadCheckpoint restores the analyzer state from the given directory.
func (t *HistoryAnalyzer) LoadCheckpoint(dir string) error {
	return newPersister().Load(dir, t.restoreFromCheckpoint)
}

// buildCheckpointState creates a serializable snapshot of the analyzer state.
func (t *HistoryAnalyzer) buildCheckpointState() *checkpointState {
	typos := make([]typoCheckpoint, len(t.typos))

	for i, typo := range t.typos {
		typos[i] = typoCheckpoint{
			Wrong:   typo.Wrong,
			Correct: typo.Correct,
			File:    typo.File,
			Commit:  hex.EncodeToString(typo.Commit[:]),
			Line:    typo.Line,
		}
	}

	return &checkpointState{
		Typos: typos,
	}
}

// restoreFromCheckpoint restores analyzer state from a checkpoint.
func (t *HistoryAnalyzer) restoreFromCheckpoint(state *checkpointState) {
	t.typos = make([]Typo, len(state.Typos))

	for i, tc := range state.Typos {
		var hash gitlib.Hash

		decoded, err := hex.DecodeString(tc.Commit)
		if err == nil && len(decoded) == len(hash) {
			copy(hash[:], decoded)
		}

		t.typos[i] = Typo{
			Wrong:   tc.Wrong,
			Correct: tc.Correct,
			File:    tc.File,
			Commit:  hash,
			Line:    tc.Line,
		}
	}
}

// Checkpoint size estimation constants.
const (
	typosBaseOverheadBytes = 100
	bytesPerTypo           = 80 // Estimated average size per typo entry.
)

// CheckpointSize returns an estimated size of the checkpoint in bytes.
func (t *HistoryAnalyzer) CheckpointSize() int64 {
	size := int64(typosBaseOverheadBytes)

	for _, typo := range t.typos {
		size += int64(bytesPerTypo + len(typo.Wrong) + len(typo.Correct) + len(typo.File))
	}

	return size
}
