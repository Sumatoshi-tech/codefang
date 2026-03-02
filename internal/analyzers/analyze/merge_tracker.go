package analyze

import (
	"github.com/Sumatoshi-tech/codefang/pkg/alg/bloom"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

// Merge tracker sizing constants.
const (
	// mergeTrackerExpected is the expected number of merge commits per chunk.
	// Merge commits are typically 5-20% of total commits; with chunk sizes
	// of ~3000 commits this gives a comfortable upper bound.
	mergeTrackerExpected = 1000

	// mergeTrackerFP is the false-positive rate for the merge tracker.
	// A false positive means skipping a legitimate merge commit, so we
	// use a very low rate to minimize data loss.
	mergeTrackerFP = 0.001
)

// MergeTracker deduplicates merge commits using a Bloom filter.
// It replaces the per-analyzer map[gitlib.Hash]bool pattern with a
// memory-efficient probabilistic structure.
//
// A false positive (rate ≤ 0.1%) means a merge commit is incorrectly
// considered already-seen and skipped. At 0.1% over 1000 merges, the
// expected number of wrongly skipped merges is ~1.
type MergeTracker struct {
	filter *bloom.Filter
}

// NewMergeTracker creates a new merge commit deduplication tracker.
func NewMergeTracker() *MergeTracker {
	// Error is structurally impossible: constants are valid.
	f, err := bloom.NewWithEstimates(mergeTrackerExpected, mergeTrackerFP)
	if err != nil {
		panic("analyze: merge tracker bloom filter initialization failed: " + err.Error())
	}

	return &MergeTracker{filter: f}
}

// SeenOrAdd checks if a merge commit has been seen before and marks it as seen.
// Returns true if the commit was already seen (should be skipped).
func (mt *MergeTracker) SeenOrAdd(hash gitlib.Hash) bool {
	return mt.filter.TestAndAdd(hash[:])
}

// Reset clears the tracker, allowing it to be reused for a new chunk.
func (mt *MergeTracker) Reset() {
	mt.filter.Reset()
}

// MarshalBinary encodes the tracker state for checkpoint serialization.
func (mt *MergeTracker) MarshalBinary() ([]byte, error) {
	return mt.filter.MarshalBinary()
}

// UnmarshalBinary restores tracker state from checkpoint data.
func (mt *MergeTracker) UnmarshalBinary(data []byte) error {
	return mt.filter.UnmarshalBinary(data)
}
