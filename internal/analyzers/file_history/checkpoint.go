package filehistory

import (
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/internal/checkpoint"
	pkgplumbing "github.com/Sumatoshi-tech/codefang/internal/plumbing"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
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
	Files       map[string]fileHistoryCheckpoint `json:"files"`
	MergesBloom []byte                           `json:"merges_bloom"`
}

// newPersister creates a checkpoint persister for file history analyzer.
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
	state := &checkpointState{
		Files: make(map[string]fileHistoryCheckpoint, len(h.files)),
	}

	// Convert files to serializable form.
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

	// Serialize merge tracker.
	mergesData, err := h.merges.MarshalBinary()
	if err == nil {
		state.MergesBloom = mergesData
	}

	return state
}

// restoreFromCheckpoint restores analyzer state from a checkpoint.
func (h *HistoryAnalyzer) restoreFromCheckpoint(state *checkpointState) {
	// Restore files.
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

	// Restore merges.
	if len(state.MergesBloom) > 0 {
		mt := analyze.NewMergeTracker()

		err := mt.UnmarshalBinary(state.MergesBloom)
		if err == nil {
			h.merges = mt
		} else {
			h.merges = analyze.NewMergeTracker()
		}
	} else {
		h.merges = analyze.NewMergeTracker()
	}
}

// Checkpoint size estimation constants.
const (
	fhBaseOverheadBytes = 100
	bytesPerFileEntry   = 120
	bytesPerPersonStats = 40
	bytesPerHash        = 44
)

// CheckpointSize returns an estimated size of the checkpoint in bytes.
func (h *HistoryAnalyzer) CheckpointSize() int64 {
	size := int64(fhBaseOverheadBytes)

	// Count file entries.
	for _, fh := range h.files {
		size += int64(bytesPerFileEntry)

		// Count person stats.
		if fh.People != nil {
			size += int64(len(fh.People) * bytesPerPersonStats)
		}

		// Count hashes.
		size += int64(len(fh.Hashes) * bytesPerHash)
	}

	// Merge tracker Bloom filter (fixed size).
	mergesData, err := h.merges.MarshalBinary()
	if err == nil {
		size += int64(len(mergesData))
	}

	return size
}
