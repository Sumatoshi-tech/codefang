package framework

import (
	"context"

	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

// Default configuration values for CommitStreamer.
const (
	// defaultBatchSize is the default number of commits per batch.
	defaultBatchSize = 10
	// defaultLookahead is the default number of batches to prefetch.
	defaultLookahead = 2
)

// CommitBatch represents a batch of commits for processing.
type CommitBatch struct {
	// Commits in this batch.
	Commits []*gitlib.Commit

	// StartIndex is the index of the first commit in the full sequence.
	StartIndex int

	// BatchID identifies this batch for ordering.
	BatchID int
}

// CommitStreamer iterates commits and groups them into batches
// for efficient processing.
type CommitStreamer struct {
	// BatchSize is the number of commits per batch.
	BatchSize int

	// Lookahead is the number of batches to prefetch.
	Lookahead int
}

// Stream takes a slice of commits and streams them as batches.
// The output channel is closed when all commits have been sent.
func (s *CommitStreamer) Stream(ctx context.Context, commits []*gitlib.Commit) <-chan CommitBatch {
	out := make(chan CommitBatch, s.Lookahead)

	go func() {
		defer close(out)

		batchID := 0

		for i := 0; i < len(commits); i += s.BatchSize {
			end := min(i+s.BatchSize, len(commits))

			batch := CommitBatch{
				Commits:    commits[i:end],
				StartIndex: i,
				BatchID:    batchID,
			}

			select {
			case out <- batch:
				batchID++
			case <-ctx.Done():
				return
			}
		}
	}()

	return out
}
