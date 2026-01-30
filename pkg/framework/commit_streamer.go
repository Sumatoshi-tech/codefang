package framework

import (
	"context"

	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
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

// NewCommitStreamer creates a new commit streamer with default settings.
func NewCommitStreamer() *CommitStreamer {
	return &CommitStreamer{
		BatchSize: 10,
		Lookahead: 2,
	}
}

// Stream takes a slice of commits and streams them as batches.
// The output channel is closed when all commits have been sent.
func (s *CommitStreamer) Stream(ctx context.Context, commits []*gitlib.Commit) <-chan CommitBatch {
	out := make(chan CommitBatch, s.Lookahead)

	go func() {
		defer close(out)

		batchID := 0
		for i := 0; i < len(commits); i += s.BatchSize {
			end := i + s.BatchSize
			if end > len(commits) {
				end = len(commits)
			}

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

// StreamFromIterator streams commits from a commit iterator.
// This is more memory-efficient for large repositories.
func (s *CommitStreamer) StreamFromIterator(ctx context.Context, iter *gitlib.CommitIter, limit int) <-chan CommitBatch {
	out := make(chan CommitBatch, s.Lookahead)

	go func() {
		defer close(out)
		defer iter.Close()

		batchID := 0
		startIndex := 0
		count := 0

		for {
			// Collect a batch
			batch := make([]*gitlib.Commit, 0, s.BatchSize)

			for len(batch) < s.BatchSize {
				if limit > 0 && count >= limit {
					break
				}

				commit, err := iter.Next()
				if err != nil {
					break
				}

				batch = append(batch, commit)
				count++
			}

			if len(batch) == 0 {
				return
			}

			commitBatch := CommitBatch{
				Commits:    batch,
				StartIndex: startIndex,
				BatchID:    batchID,
			}

			select {
			case out <- commitBatch:
				batchID++
				startIndex += len(batch)
			case <-ctx.Done():
				return
			}

			if limit > 0 && count >= limit {
				return
			}
		}
	}()

	return out
}

// StreamSingle streams commits one at a time (batch size = 1).
// This is compatible with the existing sequential processing model.
func (s *CommitStreamer) StreamSingle(ctx context.Context, commits []*gitlib.Commit) <-chan CommitBatch {
	out := make(chan CommitBatch, s.Lookahead)

	go func() {
		defer close(out)

		for i, commit := range commits {
			batch := CommitBatch{
				Commits:    []*gitlib.Commit{commit},
				StartIndex: i,
				BatchID:    i,
			}

			select {
			case out <- batch:
			case <-ctx.Done():
				return
			}
		}
	}()

	return out
}
