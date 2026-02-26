// Package cuckoo provides a space-efficient probabilistic set membership filter
// that supports deletion, unlike Bloom filters.
//
// A Cuckoo filter stores compact fingerprints in a hash table with two candidate
// buckets per entry. It supports Insert, Lookup, and Delete operations with O(1)
// amortized time and false-positive rates comparable to Bloom filters.
//
// This implementation uses 16-bit fingerprints and 4 entries per bucket, yielding
// approximately 2 bytes per element of storage with a false-positive rate < 0.1%.
package cuckoo

import (
	"encoding/binary"
	"errors"
	"hash/fnv"
)

// Filter configuration constants.
const (
	// bucketSize is the number of fingerprint entries per bucket.
	bucketSize = 4

	// maxKicks is the maximum number of eviction attempts during Insert.
	maxKicks = 500

	// fingerprintBits is the number of bits per fingerprint.
	fingerprintBits = 16

	// fingerprintMask masks a hash value down to fingerprintBits.
	fingerprintMask = (1 << fingerprintBits) - 1

	// emptyFingerprint indicates an empty bucket slot.
	emptyFingerprint = 0

	// minBuckets is the minimum number of buckets in the filter.
	minBuckets = 1

	// capacityHeadroom is the factor by which to overprovision buckets
	// to maintain a reasonable load factor and avoid full-filter failures.
	capacityHeadroom = 2

	// rngChoices is the number of candidate buckets for random selection.
	rngChoices = 2

	// fingerprintShift is the bit shift used to extract the fingerprint from a hash.
	fingerprintShift = 32
)

// Bit shift constants for nextPowerOfTwo.
const (
	shift1  = 1
	shift2  = 2
	shift4  = 4
	shift8  = 8
	shift16 = 16
	shift32 = 32
)

var (
	// ErrZeroCapacity is returned when capacity is zero.
	ErrZeroCapacity = errors.New("cuckoo: capacity must be positive")
)

// fingerprint is a 16-bit compact hash of an element.
type fingerprint uint16

// bucket holds bucketSize fingerprints.
type bucket [bucketSize]fingerprint

// Filter is a Cuckoo filter supporting Insert, Lookup, and Delete.
type Filter struct {
	buckets    []bucket
	numBuckets uint
	count      uint
	rng        splitmix64
}

// splitmix64 is a fast, non-cryptographic PRNG for internal random choices.
// It avoids math/rand which triggers gosec G404.
type splitmix64 struct {
	state uint64
}

// splitmix64 mixing constants.
const (
	splitmixInc    = 0x9e3779b97f4a7c15
	splitmixMix1   = 0xbf58476d1ce4e5b9
	splitmixMix2   = 0x94d049bb133111eb
	splitmixShift1 = 30
	splitmixShift2 = 27
	splitmixShift3 = 31
)

// next returns the next pseudo-random uint64.
func (r *splitmix64) next() uint64 {
	r.state += splitmixInc

	z := r.state
	z = (z ^ (z >> splitmixShift1)) * splitmixMix1
	z = (z ^ (z >> splitmixShift2)) * splitmixMix2

	return z ^ (z >> splitmixShift3)
}

// intn returns a pseudo-random int in [0, n).
func (r *splitmix64) intn(n int) int {
	return int(r.next()>>1) % n
}

// New creates a Cuckoo filter sized for the given capacity.
// The actual capacity may be slightly larger to ensure proper bucket sizing.
func New(capacity uint) (*Filter, error) {
	if capacity == 0 {
		return nil, ErrZeroCapacity
	}

	// Overprovision to maintain load factor below ~50% for good insert performance.
	numBuckets := max(nextPowerOfTwo((capacity*capacityHeadroom)/bucketSize), minBuckets)

	return &Filter{
		buckets:    make([]bucket, numBuckets),
		numBuckets: numBuckets,
		rng:        splitmix64{state: cuckooSeed},
	}, nil
}

// cuckooSeed is the PRNG seed for deterministic eviction ordering.
const cuckooSeed = 0x2545_F491_4F6C_DD1D

// Insert adds an element to the filter. Returns false if the filter is full
// and the element could not be inserted after maxKicks eviction attempts.
func (f *Filter) Insert(data []byte) bool {
	fp, i1 := f.fingerprintAndIndex(data)
	i2 := f.altIndex(i1, fp)

	if f.buckets[i1].insert(fp) || f.buckets[i2].insert(fp) {
		f.count++

		return true
	}

	return f.kickInsert(fp, i1, i2)
}

// Lookup returns true if the element might be in the filter (possible false positive),
// or false if the element is definitely not in the filter.
func (f *Filter) Lookup(data []byte) bool {
	fp, i1 := f.fingerprintAndIndex(data)
	i2 := f.altIndex(i1, fp)

	return f.buckets[i1].contains(fp) || f.buckets[i2].contains(fp)
}

// Delete removes an element from the filter. Returns true if the element was found
// and removed, false otherwise.
func (f *Filter) Delete(data []byte) bool {
	fp, i1 := f.fingerprintAndIndex(data)
	i2 := f.altIndex(i1, fp)

	if f.buckets[i1].remove(fp) {
		f.count--

		return true
	}

	if f.buckets[i2].remove(fp) {
		f.count--

		return true
	}

	return false
}

// Count returns the number of elements stored in the filter.
func (f *Filter) Count() uint {
	return f.count
}

// Reset clears all elements from the filter without reallocating memory.
func (f *Filter) Reset() {
	for i := range f.buckets {
		f.buckets[i] = bucket{}
	}

	f.count = 0
}

// LoadFactor returns the current occupancy ratio of the filter (0.0 to 1.0).
func (f *Filter) LoadFactor() float64 {
	totalSlots := f.numBuckets * bucketSize

	return float64(f.count) / float64(totalSlots)
}

// Capacity returns the total number of fingerprint slots in the filter.
func (f *Filter) Capacity() uint {
	return f.numBuckets * bucketSize
}

// --- Internal methods ---.

// fingerprintAndIndex computes the fingerprint and primary bucket index for data.
func (f *Filter) fingerprintAndIndex(data []byte) (fp fingerprint, idx uint) {
	h := fnvHash64(data)

	fp = deriveFingerprint(h)
	idx = uint(h) % f.numBuckets

	return fp, idx
}

// altIndex computes the alternate bucket index using partial-key cuckoo hashing.
// The alternate index is: i XOR hash(fingerprint), ensuring symmetry:
// altIndex(altIndex(i, fp), fp) == i.
func (f *Filter) altIndex(idx uint, fp fingerprint) uint {
	fpHash := fnvHashFingerprint(fp)

	return (idx ^ uint(fpHash)) % f.numBuckets
}

// kickInsert performs cuckoo eviction to make room for a new fingerprint.
func (f *Filter) kickInsert(fp fingerprint, i1, i2 uint) bool {
	// Choose a random bucket to start evictions.
	idx := i1
	if f.rng.intn(rngChoices) == 0 {
		idx = i2
	}

	for range maxKicks {
		// Swap the fingerprint with a random existing entry.
		slotIdx := f.rng.intn(bucketSize)
		fp, f.buckets[idx][slotIdx] = f.buckets[idx][slotIdx], fp

		// Try to place the evicted fingerprint in its alternate bucket.
		idx = f.altIndex(idx, fp)

		if f.buckets[idx].insert(fp) {
			f.count++

			return true
		}
	}

	return false
}

// --- Bucket operations ---.

// insert adds a fingerprint to the bucket if there is room.
func (b *bucket) insert(fp fingerprint) bool {
	for i := range b {
		if b[i] == emptyFingerprint {
			b[i] = fp

			return true
		}
	}

	return false
}

// contains returns true if the bucket contains the given fingerprint.
func (b *bucket) contains(fp fingerprint) bool {
	for _, entry := range b {
		if entry == fp {
			return true
		}
	}

	return false
}

// remove removes the first occurrence of the fingerprint from the bucket.
func (b *bucket) remove(fp fingerprint) bool {
	for i := range b {
		if b[i] == fp {
			b[i] = emptyFingerprint

			return true
		}
	}

	return false
}

// --- Hash functions ---.

// fnvHash64 computes a 64-bit FNV-1a hash of the data.
func fnvHash64(data []byte) uint64 {
	h := fnv.New64a()
	h.Write(data)

	return h.Sum64()
}

// fnvHashFingerprint computes a hash of a fingerprint for alternate index calculation.
func fnvHashFingerprint(fp fingerprint) uint64 {
	var buf [fingerprintBytes]byte

	binary.LittleEndian.PutUint16(buf[:], uint16(fp))

	h := fnv.New64a()
	h.Write(buf[:])

	return h.Sum64()
}

// fingerprintBytes is the number of bytes needed to encode a fingerprint.
const fingerprintBytes = 2

// deriveFingerprint extracts a non-zero fingerprint from a hash value.
// Uses the upper bits to derive the fingerprint, ensuring it is never zero.
func deriveFingerprint(h uint64) fingerprint {
	fp := fingerprint((h >> fingerprintShift) & fingerprintMask)
	if fp == emptyFingerprint {
		fp = 1
	}

	return fp
}

// nextPowerOfTwo returns the smallest power of two >= n.
func nextPowerOfTwo(n uint) uint {
	if n == 0 {
		return 1
	}

	n--
	n |= n >> shift1
	n |= n >> shift2
	n |= n >> shift4
	n |= n >> shift8
	n |= n >> shift16
	n |= n >> shift32
	n++

	return n
}

// intToBytes converts an integer to a byte slice for hashing.
func intToBytes(n int) []byte {
	buf := make([]byte, binary.MaxVarintLen64)
	written := binary.PutVarint(buf, int64(n))

	return buf[:written]
}
