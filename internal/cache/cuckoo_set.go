package cache

import (
	"fmt"

	"github.com/Sumatoshi-tech/codefang/pkg/alg/cuckoo"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

// CuckooHashSet is a probabilistic hash set backed by a Cuckoo filter.
// It provides the same API shape as BloomHashSet but additionally supports
// Remove, making it suitable for incremental analysis where processed blobs
// may need to be "forgotten" after file renames or removals.
type CuckooHashSet struct {
	filter *cuckoo.Filter
}

// NewCuckooHashSet creates a Cuckoo-backed hash set sized for expectedElements.
// Returns an error if expectedElements is zero.
func NewCuckooHashSet(expectedElements uint) (*CuckooHashSet, error) {
	f, err := cuckoo.New(expectedElements)
	if err != nil {
		return nil, fmt.Errorf("cuckoo hash set: %w", err)
	}

	return &CuckooHashSet{filter: f}, nil
}

// Add inserts a hash into the set. Returns true if the insertion succeeded.
// Returns false if the filter is full and the hash could not be inserted.
func (s *CuckooHashSet) Add(hash gitlib.Hash) bool {
	return s.filter.Insert(hash[:])
}

// Contains reports whether the hash is possibly in the set. A return value
// of false guarantees the hash was never added (or was removed). A return
// value of true means the hash might be present (subject to the FP rate).
func (s *CuckooHashSet) Contains(hash gitlib.Hash) bool {
	return s.filter.Lookup(hash[:])
}

// Remove deletes a hash from the set. Returns true if the hash was found
// and removed, false otherwise.
func (s *CuckooHashSet) Remove(hash gitlib.Hash) bool {
	return s.filter.Delete(hash[:])
}

// Len returns the number of elements in the set.
func (s *CuckooHashSet) Len() uint {
	return s.filter.Count()
}

// Clear resets the set, removing all elements without reallocating memory.
func (s *CuckooHashSet) Clear() {
	s.filter.Reset()
}

// LoadFactor returns the current occupancy ratio of the underlying Cuckoo
// filter, in the range [0.0, 1.0].
func (s *CuckooHashSet) LoadFactor() float64 {
	return s.filter.LoadFactor()
}
