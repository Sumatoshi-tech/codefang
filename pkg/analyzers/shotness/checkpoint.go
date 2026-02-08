package shotness

import (
	"github.com/Sumatoshi-tech/codefang/pkg/checkpoint"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

// checkpointBasename is the base filename for checkpoint files.
const checkpointBasename = "shotness_state"

// nodeCheckpoint is the serializable form of nodeShotness.
type nodeCheckpoint struct {
	Couples map[string]int `json:"couples"`
	Summary NodeSummary    `json:"summary"`
	Count   int            `json:"count"`
}

// checkpointState holds the serializable state of the shotness analyzer.
type checkpointState struct {
	Nodes  map[string]nodeCheckpoint `json:"nodes"`
	Merges []string                  `json:"merges"`
}

// newPersister creates a checkpoint persister for shotness analyzer.
func newPersister() *checkpoint.Persister[checkpointState] {
	return checkpoint.NewPersister[checkpointState](
		checkpointBasename,
		checkpoint.NewJSONCodec(),
	)
}

// SaveCheckpoint writes the analyzer state to the given directory.
func (s *HistoryAnalyzer) SaveCheckpoint(dir string) error {
	return newPersister().Save(dir, s.buildCheckpointState)
}

// LoadCheckpoint restores the analyzer state from the given directory.
func (s *HistoryAnalyzer) LoadCheckpoint(dir string) error {
	return newPersister().Load(dir, s.restoreFromCheckpoint)
}

// buildCheckpointState creates a serializable snapshot of the analyzer state.
func (s *HistoryAnalyzer) buildCheckpointState() *checkpointState {
	state := &checkpointState{
		Nodes:  make(map[string]nodeCheckpoint, len(s.nodes)),
		Merges: make([]string, 0, len(s.merges)),
	}

	// Convert nodes to serializable form
	for key, ns := range s.nodes {
		state.Nodes[key] = nodeCheckpoint{
			Couples: ns.Couples,
			Summary: ns.Summary,
			Count:   ns.Count,
		}
	}

	// Convert merge hashes to strings
	for hash := range s.merges {
		state.Merges = append(state.Merges, hash.String())
	}

	return state
}

// restoreFromCheckpoint restores analyzer state from a checkpoint.
func (s *HistoryAnalyzer) restoreFromCheckpoint(state *checkpointState) {
	// Restore nodes
	s.nodes = make(map[string]*nodeShotness, len(state.Nodes))

	for key, nc := range state.Nodes {
		s.nodes[key] = &nodeShotness{
			Couples: nc.Couples,
			Summary: nc.Summary,
			Count:   nc.Count,
		}
	}

	// Restore merges
	s.merges = make(map[gitlib.Hash]bool, len(state.Merges))

	for _, hashStr := range state.Merges {
		s.merges[gitlib.NewHash(hashStr)] = true
	}

	// Rebuild files map from nodes
	s.rebuildFilesMap()
}

// Checkpoint size estimation constants.
const (
	shBaseOverheadBytes = 100
	bytesPerNode        = 150
	bytesPerCouple      = 50
	bytesPerMergeHash   = 44
)

// CheckpointSize returns an estimated size of the checkpoint in bytes.
func (s *HistoryAnalyzer) CheckpointSize() int64 {
	size := int64(shBaseOverheadBytes)

	// Count nodes
	for _, ns := range s.nodes {
		size += int64(bytesPerNode)
		size += int64(len(ns.Couples) * bytesPerCouple)
	}

	// Count merge entries
	size += int64(len(s.merges) * bytesPerMergeHash)

	return size
}
