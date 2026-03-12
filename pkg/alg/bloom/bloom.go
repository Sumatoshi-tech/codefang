// Package bloom provides a space-efficient probabilistic set membership filter.
//
// A Bloom filter answers "definitely not in set" or "possibly in set" with a
// tunable false-positive rate. It is useful as a pre-filter to avoid expensive
// exact lookups (map access, lock acquisition, disk I/O).
//
// This implementation uses the double-hashing technique from Kirsch and
// Mitzenmacher (2006): two base hashes derive k bit positions via
// h(i) = h1 + i*h2 mod m, avoiding k independent hash functions.
package bloom

import (
	"encoding/binary"
	"errors"
	"hash/fnv"
	"math"
	"math/bits"
	"sync"
)

// Sentinel errors for binary deserialization.
var (
	errBinaryDataTooShort    = errors.New("bloom: binary data too short")
	errBinaryDataLenMismatch = errors.New("bloom: binary data length mismatch")
)

const (
	// bitsPerWord is the number of bits in each uint64 word.
	bitsPerWord = 64

	// ln2Squared is ln(2) squared, used in the optimal bit-array size formula.
	ln2Squared = math.Ln2 * math.Ln2

	// bloomHeaderSize is the byte size of the serialized header (m + k + count).
	bloomHeaderSize = 24

	// uint64Size is the byte size of a single uint64 word.
	uint64Size = 8
)

var (
	// ErrZeroN is returned when n (expected element count) is zero.
	ErrZeroN = errors.New("bloom: n must be positive")

	// ErrInvalidFP is returned when fp is not in the open interval (0, 1).
	ErrInvalidFP = errors.New("bloom: fp must be in the open interval (0, 1)")
)

// Filter is a thread-safe Bloom filter.
type Filter struct {
	mu    sync.RWMutex
	bits  []uint64
	m     uint // Total bits.
	k     uint // Number of hash functions.
	count uint // Approximate number of added elements.
}

// NewWithEstimates creates a Bloom filter sized for n expected elements at a
// false-positive rate of fp. Returns an error if n is zero or fp is not in the
// open interval (0, 1).
func NewWithEstimates(n uint, fp float64) (*Filter, error) {
	if n == 0 {
		return nil, ErrZeroN
	}

	if fp <= 0 || fp >= 1 {
		return nil, ErrInvalidFP
	}

	m := optimalM(n, fp)
	k := optimalK(m, n)
	words := (m + bitsPerWord - 1) / bitsPerWord

	return &Filter{
		bits: make([]uint64, words),
		m:    m,
		k:    k,
	}, nil
}

// BitCount returns the size of the bit array in bits.
func (f *Filter) BitCount() uint {
	return f.m
}

// HashCount returns the number of hash functions used by the filter.
func (f *Filter) HashCount() uint {
	return f.k
}

// Add inserts data into the filter.
func (f *Filter) Add(data []byte) {
	h1, h2 := hashKernel(data)

	f.mu.Lock()
	setBits(f.bits, f.m, f.k, h1, h2)

	f.count++
	f.mu.Unlock()
}

// Test reports whether data is possibly in the filter. A return value of false
// guarantees the element was never added. A return value of true means the
// element might have been added (subject to the configured false-positive rate).
func (f *Filter) Test(data []byte) bool {
	h1, h2 := hashKernel(data)

	f.mu.RLock()
	defer f.mu.RUnlock()

	return testBits(f.bits, f.m, f.k, h1, h2)
}

// TestAndAdd tests for membership and then adds the element. It returns true if
// the element was possibly already present before this call.
func (f *Filter) TestAndAdd(data []byte) bool {
	h1, h2 := hashKernel(data)

	f.mu.Lock()
	defer f.mu.Unlock()

	present := true

	for i := range f.k {
		pos := (h1 + uint64(i)*h2) % uint64(f.m)
		wordIdx := pos / bitsPerWord
		bitMask := uint64(1) << (pos % bitsPerWord)

		if f.bits[wordIdx]&bitMask == 0 {
			present = false
			f.bits[wordIdx] |= bitMask
		}
	}

	f.count++

	return present
}

// AddBulk inserts multiple elements into the filter.
func (f *Filter) AddBulk(items [][]byte) {
	if len(items) == 0 {
		return
	}

	f.mu.Lock()
	for _, item := range items {
		h1, h2 := hashKernel(item)
		setBits(f.bits, f.m, f.k, h1, h2)

		f.count++
	}
	f.mu.Unlock()
}

// TestBulk tests multiple elements for membership. Returns a bool slice of the
// same length as items, where each entry indicates possible presence.
func (f *Filter) TestBulk(items [][]byte) []bool {
	if len(items) == 0 {
		return nil
	}

	results := make([]bool, len(items))

	f.mu.RLock()

	for idx, item := range items {
		h1, h2 := hashKernel(item)
		results[idx] = testBits(f.bits, f.m, f.k, h1, h2)
	}

	f.mu.RUnlock()

	return results
}

// EstimatedCount returns an approximation of the number of elements that have
// been added to the filter.
func (f *Filter) EstimatedCount() uint {
	f.mu.RLock()
	defer f.mu.RUnlock()

	return f.count
}

// FillRatio returns the fraction of bits that are set, in the range [0, 1].
func (f *Filter) FillRatio() float64 {
	f.mu.RLock()
	defer f.mu.RUnlock()

	total := uint(0)
	for _, word := range f.bits {
		total += uint(bits.OnesCount64(word))
	}

	return float64(total) / float64(f.m)
}

// MarshalBinary encodes the filter into a binary format.
// Layout: [m uint64][k uint64][count uint64][bits...].
func (f *Filter) MarshalBinary() ([]byte, error) {
	f.mu.RLock()
	defer f.mu.RUnlock()

	// Header: m + k + count = 3 * 8 bytes.
	buf := make([]byte, bloomHeaderSize+len(f.bits)*uint64Size)
	binary.BigEndian.PutUint64(buf[0:uint64Size], uint64(f.m))
	binary.BigEndian.PutUint64(buf[uint64Size:2*uint64Size], uint64(f.k))
	binary.BigEndian.PutUint64(buf[2*uint64Size:bloomHeaderSize], uint64(f.count))

	for i, word := range f.bits {
		binary.BigEndian.PutUint64(buf[bloomHeaderSize+i*uint64Size:bloomHeaderSize+(i+1)*uint64Size], word)
	}

	return buf, nil
}

// UnmarshalBinary decodes the filter from a binary format produced by MarshalBinary.
func (f *Filter) UnmarshalBinary(data []byte) error {
	if len(data) < bloomHeaderSize {
		return errBinaryDataTooShort
	}

	m := binary.BigEndian.Uint64(data[0:uint64Size])
	k := binary.BigEndian.Uint64(data[uint64Size : 2*uint64Size])
	count := binary.BigEndian.Uint64(data[2*uint64Size : bloomHeaderSize])

	words := (m + bitsPerWord - 1) / bitsPerWord

	if uint64(len(data)-bloomHeaderSize) != words*uint64Size {
		return errBinaryDataLenMismatch
	}

	bitsArr := make([]uint64, words)
	for i := range bitsArr {
		bitsArr[i] = binary.BigEndian.Uint64(data[bloomHeaderSize+i*uint64Size : bloomHeaderSize+(i+1)*uint64Size])
	}

	f.mu.Lock()
	f.m = uint(m)
	f.k = uint(k)
	f.count = uint(count)
	f.bits = bitsArr
	f.mu.Unlock()

	return nil
}

// Reset clears the filter without reallocating the bit array.
func (f *Filter) Reset() {
	f.mu.Lock()
	for i := range f.bits {
		f.bits[i] = 0
	}

	f.count = 0
	f.mu.Unlock()
}

// setBits sets the k bit positions derived from h1 and h2 in the bit array.
func setBits(arr []uint64, m, k uint, h1, h2 uint64) {
	for i := range k {
		pos := (h1 + uint64(i)*h2) % uint64(m)
		arr[pos/bitsPerWord] |= 1 << (pos % bitsPerWord)
	}
}

// testBits returns true if all k bit positions derived from h1 and h2 are set.
func testBits(arr []uint64, m, k uint, h1, h2 uint64) bool {
	for i := range k {
		pos := (h1 + uint64(i)*h2) % uint64(m)
		if arr[pos/bitsPerWord]&(1<<(pos%bitsPerWord)) == 0 {
			return false
		}
	}

	return true
}

// optimalM computes the optimal bit-array size for n elements at false-positive
// rate fp using the formula m = ceil(-n * ln(fp) / ln(2)^2).
func optimalM(n uint, fp float64) uint {
	return uint(math.Ceil(-float64(n) * math.Log(fp) / ln2Squared))
}

// optimalK computes the optimal number of hash functions using the formula
// k = round(m/n * ln(2)).
func optimalK(m, n uint) uint {
	k := uint(math.Round(float64(m) / float64(n) * math.Ln2))
	if k < 1 {
		return 1
	}

	return k
}

// hashKernel computes two independent 64-bit hashes from data using FNV-128a.
// The 128-bit digest is split into two 64-bit halves. The second half is forced
// odd so the step through the bit array is coprime with any even m.
func hashKernel(data []byte) (h1, h2 uint64) {
	h := fnv.New128a()
	_, _ = h.Write(data)
	sum := h.Sum(nil)

	h1 = binary.BigEndian.Uint64(sum[:8])
	h2 = binary.BigEndian.Uint64(sum[8:])

	// Force h2 odd so gcd(h2, m) avoids degenerate cycling.
	h2 |= 1

	return h1, h2
}
