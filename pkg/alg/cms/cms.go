// Package cms provides a Count-Min Sketch for frequency estimation.
//
// A Count-Min Sketch estimates the frequency of elements in a data stream
// using bounded overestimation. It answers "how many times has this element
// been seen?" with an estimate that is always >= the true count (for
// positive-only additions) and bounded by epsilon * totalCount with
// probability >= 1 - delta.
//
// This implementation uses multiple independent hash functions derived from
// FNV-1a with per-row seeds mixed via a splitmix64 finalizer.
package cms

import (
	"encoding/binary"
	"errors"
	"hash/fnv"
	"math"
	"sync"
)

const (
	// baseSeed is the starting seed for deterministic seed generation.
	baseSeed = 0x517cc1b727220a95

	// mixShift1 is the first shift in the splitmix64 finalizer.
	mixShift1 = 30

	// mixMul1 is the first multiplier in the splitmix64 finalizer.
	mixMul1 = 0xbf58476d1ce4e5b9

	// mixShift2 is the second shift in the splitmix64 finalizer.
	mixShift2 = 27

	// mixMul2 is the second multiplier in the splitmix64 finalizer.
	mixMul2 = 0x94d049bb133111eb

	// mixShift3 is the third shift in the splitmix64 finalizer.
	mixShift3 = 31
)

var (
	// ErrInvalidEpsilon is returned when epsilon is not positive.
	ErrInvalidEpsilon = errors.New("cms: epsilon must be positive")

	// ErrInvalidDelta is returned when delta is not in the open interval (0, 1).
	ErrInvalidDelta = errors.New("cms: delta must be in the open interval (0, 1)")
)

// Sketch is a thread-safe Count-Min Sketch for frequency estimation.
type Sketch struct {
	mu         sync.RWMutex
	counters   []int64  // Flattened 2D array: depth rows × width columns.
	seeds      []uint64 // One seed per row for independent hashing.
	width      uint
	depth      uint
	totalCount int64
}

// New creates a Count-Min Sketch with automatic sizing from the desired
// error bounds. Width = ceil(e / epsilon), depth = ceil(ln(1 / delta)).
// Returns an error if epsilon <= 0 or delta is not in (0, 1).
func New(epsilon, delta float64) (*Sketch, error) {
	if epsilon <= 0 {
		return nil, ErrInvalidEpsilon
	}

	if delta <= 0 || delta >= 1 {
		return nil, ErrInvalidDelta
	}

	width := uint(math.Ceil(math.E / epsilon))
	depth := uint(math.Ceil(math.Log(1 / delta)))
	seeds := generateSeeds(depth)

	return &Sketch{
		counters: make([]int64, width*depth),
		seeds:    seeds,
		width:    width,
		depth:    depth,
	}, nil
}

// Width returns the number of columns in the sketch.
func (s *Sketch) Width() uint {
	return s.width
}

// Depth returns the number of rows (hash functions) in the sketch.
func (s *Sketch) Depth() uint {
	return s.depth
}

// Add increments the counter for key by count. A count of zero is a no-op.
func (s *Sketch) Add(key []byte, count int64) {
	if count == 0 {
		return
	}

	s.mu.Lock()

	for row := range s.depth {
		col := s.hashKey(row, key)
		s.counters[row*s.width+col] += count
	}

	s.totalCount += count

	s.mu.Unlock()
}

// Count returns the estimated frequency of key. For positive-only additions,
// the estimate is always >= the true count.
func (s *Sketch) Count(key []byte) int64 {
	s.mu.RLock()
	defer s.mu.RUnlock()

	minVal := int64(math.MaxInt64)

	for row := range s.depth {
		col := s.hashKey(row, key)
		val := s.counters[row*s.width+col]

		if val < minVal {
			minVal = val
		}
	}

	return minVal
}

// TotalCount returns the sum of all counts added to the sketch.
func (s *Sketch) TotalCount() int64 {
	s.mu.RLock()
	defer s.mu.RUnlock()

	return s.totalCount
}

// Reset clears all counters and the total count without reallocation.
func (s *Sketch) Reset() {
	s.mu.Lock()

	for i := range s.counters {
		s.counters[i] = 0
	}

	s.totalCount = 0

	s.mu.Unlock()
}

// hashKey computes the column index for the given row and key.
func (s *Sketch) hashKey(row uint, key []byte) uint {
	h := fnv.New64a()

	// Write the seed as an 8-byte prefix for per-row independence.
	var seedBuf [8]byte

	binary.LittleEndian.PutUint64(seedBuf[:], s.seeds[row])

	_, _ = h.Write(seedBuf[:])
	_, _ = h.Write(key)

	return uint(h.Sum64()) % s.width
}

// generateSeeds creates depth deterministic seeds using splitmix64.
func generateSeeds(depth uint) []uint64 {
	seeds := make([]uint64, depth)
	state := uint64(baseSeed)

	for i := range depth {
		state = mix64(state)
		seeds[i] = state
	}

	return seeds
}

// mix64 applies a splitmix64-style finalizer for full-avalanche mixing.
func mix64(v uint64) uint64 {
	v ^= v >> mixShift1
	v *= mixMul1
	v ^= v >> mixShift2
	v *= mixMul2
	v ^= v >> mixShift3

	return v
}
