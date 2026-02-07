package filehistory

import (
	"github.com/Sumatoshi-tech/codefang/pkg/checkpoint"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
	pkgplumbing "github.com/Sumatoshi-tech/codefang/pkg/plumbing"
)

// checkpointBasename is the base filename for checkpoint files.
const checkpointBasename = "file_history_state"

// fileHistoryCheckpoint is the serializable form of FileHistory.
type fileHistoryCheckpoint struct {
	People map[int]pkgplumbing.LineStats `json:"people"`
	Hashes []string                      `json:"hashes"`
}

// checkpointState holds the serializable state of the file history analyzer.
type checkpointState struct {
	Files  map[string]fileHistoryCheckpoint `json:"files"`
	Merges []string                         `json:"merges"`
}

// newPersister creates a checkpoint persister for file history analyzer.
func newPersister() *checkpoint.Persister[checkpointState] {
	return checkpoint.NewPersister[checkpointState](
		checkpointBasename,
		checkpoint.NewJSONCodec(),
	)
}

// SaveCheckpoint writes the analyzer state to the given directory.
func (h *Analyzer) SaveCheckpoint(dir string) error {
	return newPersister().Save(dir, h.buildCheckpointState)
}

// LoadCheckpoint restores the analyzer state from the given directory.
func (h *Analyzer) LoadCheckpoint(dir string) error {
	return newPersister().Load(dir, h.restoreFromCheckpoint)
}

// buildCheckpointState creates a serializable snapshot of the analyzer state.
func (h *Analyzer) buildCheckpointState() *checkpointState {
	state := &checkpointState{
		Files:  make(map[string]fileHistoryCheckpoint, len(h.files)),
		Merges: make([]string, 0, len(h.merges)),
	}

	// Convert files to serializable form
	for name, fh := range h.files {
		cp := fileHistoryCheckpoint{
			People: fh.People,
			Hashes: make([]string, len(fh.Hashes)),
		}

		for i, hash := range fh.Hashes {
			cp.Hashes[i] = hash.String()
		}

		state.Files[name] = cp
	}

	// Convert merge hashes to strings
	for hash := range h.merges {
		state.Merges = append(state.Merges, hash.String())
	}

	return state
}

// restoreFromCheckpoint restores analyzer state from a checkpoint.
func (h *Analyzer) restoreFromCheckpoint(state *checkpointState) {
	// Restore files
	h.files = make(map[string]*FileHistory, len(state.Files))
	for name, cp := range state.Files {
		fh := &FileHistory{
			People: cp.People,
			Hashes: make([]gitlib.Hash, len(cp.Hashes)),
		}

		for i, hashStr := range cp.Hashes {
			fh.Hashes[i] = gitlib.NewHash(hashStr)
		}

		h.files[name] = fh
	}

	// Restore merges
	h.merges = make(map[gitlib.Hash]bool, len(state.Merges))
	for _, hashStr := range state.Merges {
		h.merges[gitlib.NewHash(hashStr)] = true
	}
}

// Checkpoint size estimation constants.
const (
	fhBaseOverheadBytes = 100
	bytesPerFileEntry   = 120
	bytesPerPersonStats = 40
	bytesPerHash        = 44
	bytesPerMergeEntry  = 44
)

// CheckpointSize returns an estimated size of the checkpoint in bytes.
func (h *Analyzer) CheckpointSize() int64 {
	size := int64(fhBaseOverheadBytes)

	// Count file entries
	for _, fh := range h.files {
		size += int64(bytesPerFileEntry)

		// Count person stats
		if fh.People != nil {
			size += int64(len(fh.People) * bytesPerPersonStats)
		}

		// Count hashes
		size += int64(len(fh.Hashes) * bytesPerHash)
	}

	// Count merge entries
	size += int64(len(h.merges) * bytesPerMergeEntry)

	return size
}
