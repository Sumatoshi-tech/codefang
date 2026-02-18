package sentiment

import (
	"github.com/Sumatoshi-tech/codefang/pkg/checkpoint"
)

// checkpointBasename is the base filename for checkpoint files.
const checkpointBasename = "sentiment_state"

// checkpointState holds the serializable state of the sentiment analyzer.
type checkpointState struct {
	CommentsByCommit map[string][]string `json:"comments_by_commit"`
}

// newPersister creates a checkpoint persister for sentiment analyzer.
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
	return &checkpointState{
		CommentsByCommit: s.commentsByCommit,
	}
}

// restoreFromCheckpoint restores analyzer state from a checkpoint.
func (s *HistoryAnalyzer) restoreFromCheckpoint(state *checkpointState) {
	s.commentsByCommit = state.CommentsByCommit
}

// Checkpoint size estimation constants.
const (
	sentimentBaseOverheadBytes = 100
	bytesPerCommitEntry        = 60
	bytesPerComment            = 50
)

// CheckpointSize returns an estimated size of the checkpoint in bytes.
func (s *HistoryAnalyzer) CheckpointSize() int64 {
	size := int64(sentimentBaseOverheadBytes)

	for _, comments := range s.commentsByCommit {
		size += int64(bytesPerCommitEntry)

		for _, comment := range comments {
			size += int64(bytesPerComment + len(comment))
		}
	}

	return size
}
