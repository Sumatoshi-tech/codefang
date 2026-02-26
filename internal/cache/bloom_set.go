package cache

import (
	"fmt"

	"github.com/Sumatoshi-tech/codefang/pkg/alg/bloom"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

// BloomHashSet is a probabilistic hash set backed by a Bloom filter.
// It provides the same API shape as HashSet but uses constant memory
// regardless of element count, trading exact membership for a configurable
// false-positive rate with zero false negatives.
//
// Thread-safety is inherited from the underlying bloom.Filter.
type BloomHashSet struct {
	filter *bloom.Filter
}

// NewBloomHashSet creates a Bloom-backed hash set sized for expectedElements
// at the given false-positive rate. Returns an error if expectedElements is
// zero or fpRate is not in the open interval (0, 1).
func NewBloomHashSet(expectedElements uint, fpRate float64) (*BloomHashSet, error) {
	bf, err := bloom.NewWithEstimates(expectedElements, fpRate)
	if err != nil {
		return nil, fmt.Errorf("bloom hash set: %w", err)
	}

	return &BloomHashSet{filter: bf}, nil
}

// Add inserts a hash into the set. Returns true if the hash was definitely
// not present before this call (zero false negatives). Returns false if the
// hash was possibly already present (subject to the configured FP rate).
func (s *BloomHashSet) Add(hash gitlib.Hash) bool {
	wasPresent := s.filter.TestAndAdd(hash[:])

	return !wasPresent
}

// Contains reports whether the hash is possibly in the set. A return value
// of false guarantees the hash was never added. A return value of true means
// the hash might have been added (subject to the configured FP rate).
func (s *BloomHashSet) Contains(hash gitlib.Hash) bool {
	return s.filter.Test(hash[:])
}

// Len returns the approximate number of elements added to the set.
// This count may over-estimate if the same hash is added multiple times.
func (s *BloomHashSet) Len() uint {
	return s.filter.EstimatedCount()
}

// Clear resets the set, removing all elements without reallocating
// the underlying bit array.
func (s *BloomHashSet) Clear() {
	s.filter.Reset()
}

// FillRatio returns the fraction of bits set in the underlying Bloom filter,
// in the range [0, 1]. Useful for monitoring filter saturation.
func (s *BloomHashSet) FillRatio() float64 {
	return s.filter.FillRatio()
}
