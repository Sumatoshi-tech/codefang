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

// NewCommitStreamer creates a new commit streamer with default settings.
func NewCommitStreamer() *CommitStreamer {
	return &CommitStreamer{
		BatchSize: defaultBatchSize,
		Lookahead: defaultLookahead,
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

// iteratorStreamState holds state for streaming from an iterator.
type iteratorStreamState struct {
	streamer   *CommitStreamer
	iter       *gitlib.CommitIter
	out        chan<- CommitBatch
	limit      int
	batchID    int
	startIndex int
	count      int
}

// collectBatch collects up to BatchSize commits from the iterator.
func (st *iteratorStreamState) collectBatch() []*gitlib.Commit {
	batch := make([]*gitlib.Commit, 0, st.streamer.BatchSize)

	for len(batch) < st.streamer.BatchSize {
		if st.limit > 0 && st.count >= st.limit {
			break
		}

		commit, err := st.iter.Next()
		if err != nil {
			break
		}

		batch = append(batch, commit)
		st.count++
	}

	return batch
}

// sendBatch sends a batch to the output channel.
func (st *iteratorStreamState) sendBatch(ctx context.Context, batch []*gitlib.Commit) bool {
	commitBatch := CommitBatch{
		Commits:    batch,
		StartIndex: st.startIndex,
		BatchID:    st.batchID,
	}

	select {
	case st.out <- commitBatch:
		st.batchID++
		st.startIndex += len(batch)

		return true
	case <-ctx.Done():
		return false
	}
}

// StreamFromIterator streams commits from a commit iterator.
// This is more memory-efficient for large repositories.
func (s *CommitStreamer) StreamFromIterator(ctx context.Context, iter *gitlib.CommitIter, limit int) <-chan CommitBatch {
	out := make(chan CommitBatch, s.Lookahead)

	go func() {
		defer close(out)
		defer iter.Close()

		st := &iteratorStreamState{streamer: s, iter: iter, out: out, limit: limit}

		for {
			batch := st.collectBatch()
			if len(batch) == 0 {
				return
			}

			if !st.sendBatch(ctx, batch) {
				return
			}

			if limit > 0 && st.count >= limit {
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
