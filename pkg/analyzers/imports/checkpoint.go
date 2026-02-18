package imports

import (
	"github.com/Sumatoshi-tech/codefang/pkg/checkpoint"
)

// checkpointBasename is the base filename for checkpoint files.
const checkpointBasename = "imports_state"

// checkpointState holds the serializable state of the imports analyzer.
type checkpointState struct {
	Imports Map `json:"imports"`
}

// newPersister creates a checkpoint persister for imports analyzer.
func newPersister() *checkpoint.Persister[checkpointState] {
	return checkpoint.NewPersister[checkpointState](
		checkpointBasename,
		checkpoint.NewJSONCodec(),
	)
}

// SaveCheckpoint writes the analyzer state to the given directory.
func (h *HistoryAnalyzer) SaveCheckpoint(dir string) error {
	return newPersister().Save(dir, h.buildCheckpointState)
}

// LoadCheckpoint restores the analyzer state from the given directory.
func (h *HistoryAnalyzer) LoadCheckpoint(dir string) error {
	return newPersister().Load(dir, h.restoreFromCheckpoint)
}

// buildCheckpointState creates a serializable snapshot of the analyzer state.
func (h *HistoryAnalyzer) buildCheckpointState() *checkpointState {
	return &checkpointState{
		Imports: h.imports,
	}
}

// restoreFromCheckpoint restores analyzer state from a checkpoint.
func (h *HistoryAnalyzer) restoreFromCheckpoint(state *checkpointState) {
	h.imports = state.Imports
}

// Checkpoint size estimation constants.
const (
	importsBaseOverheadBytes = 100
	bytesPerAuthor           = 30
	bytesPerLang             = 20
	bytesPerImport           = 50
	bytesPerTickEntry        = 16
)

// CheckpointSize returns an estimated size of the checkpoint in bytes.
func (h *HistoryAnalyzer) CheckpointSize() int64 {
	size := int64(importsBaseOverheadBytes)

	for _, langs := range h.imports {
		size += int64(bytesPerAuthor)

		for _, imps := range langs {
			size += int64(bytesPerLang)

			for _, ticks := range imps {
				size += int64(bytesPerImport)
				size += int64(len(ticks) * bytesPerTickEntry)
			}
		}
	}

	return size
}
