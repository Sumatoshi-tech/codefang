// Package minhash provides MinHash signature generation for set similarity estimation.
//
// MinHash compresses a set of tokens or shingles into a compact fixed-size
// signature. The Jaccard similarity between two sets can then be estimated
// by comparing signatures in O(k) time, where k is the number of hash
// functions (typically 128).
//
// This implementation uses FNV-1a base hashing with per-hash-function seeds
// mixed via a splitmix64 finalizer to produce k independent hash values from
// a single base hash computation.
package minhash

import (
	"encoding/binary"
	"errors"
	"hash/fnv"
	"math"
	"sync"
)

const (
	// baseSeed is the starting seed for deterministic per-hash seed generation.
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

	// HeaderSize is the number of bytes for the numHashes uint32 in serialization.
	HeaderSize = 4

	// BytesPerHash is the number of bytes per uint64 hash value in serialization.
	BytesPerHash = 8
)

var (
	// ErrZeroNumHashes is returned when numHashes is zero.
	ErrZeroNumHashes = errors.New("minhash: numHashes must be positive")

	// ErrSizeMismatch is returned when comparing signatures of different sizes.
	ErrSizeMismatch = errors.New("minhash: signature sizes do not match")

	// ErrNilSignature is returned when a nil signature is provided.
	ErrNilSignature = errors.New("minhash: signature must not be nil")

	// ErrInvalidData is returned when deserialization data is invalid.
	ErrInvalidData = errors.New("minhash: invalid serialized data")
)

// Signature is a thread-safe MinHash signature for Jaccard similarity estimation.
type Signature struct {
	mu    sync.Mutex
	mins  []uint64
	seeds []uint64
}

// New creates a new MinHash signature with the given number of hash functions.
// Each minimum is initialized to [math.MaxUint64]. Returns an error if
// numHashes is zero.
func New(numHashes int) (*Signature, error) {
	if numHashes <= 0 {
		return nil, ErrZeroNumHashes
	}

	mins := make([]uint64, numHashes)
	for i := range mins {
		mins[i] = math.MaxUint64
	}

	return &Signature{
		mins:  mins,
		seeds: generateSeeds(numHashes),
	}, nil
}

// Add updates all hash function minimums with the given token.
func (s *Signature) Add(token []byte) {
	baseHash := fnvHash(token)

	s.mu.Lock()
	defer s.mu.Unlock()

	for i, seed := range s.seeds {
		h := mixHash(baseHash, seed)
		if h < s.mins[i] {
			s.mins[i] = h
		}
	}
}

// Similarity returns the estimated Jaccard index between this signature and
// another. Returns an error if the signatures have different sizes or if
// other is nil.
func (s *Signature) Similarity(other *Signature) (float64, error) {
	if other == nil {
		return 0, ErrNilSignature
	}

	// Self-comparison: avoid mutex deadlock when s == other.
	if s == other {
		return 1.0, nil
	}

	s.mu.Lock()
	defer s.mu.Unlock()

	other.mu.Lock()
	defer other.mu.Unlock()

	if len(s.mins) != len(other.mins) {
		return 0, ErrSizeMismatch
	}

	matches := 0

	for i := range s.mins {
		if s.mins[i] == other.mins[i] {
			matches++
		}
	}

	return float64(matches) / float64(len(s.mins)), nil
}

// Bytes serializes the signature to a compact binary format.
// Format: [numHashes as uint32 big-endian (4 bytes)] + [mins as []uint64 big-endian].
func (s *Signature) Bytes() []byte {
	s.mu.Lock()
	defer s.mu.Unlock()

	data := make([]byte, HeaderSize+len(s.mins)*BytesPerHash)
	binary.BigEndian.PutUint32(data[:HeaderSize], uint32(len(s.mins)))

	for i, v := range s.mins {
		offset := HeaderSize + i*BytesPerHash
		binary.BigEndian.PutUint64(data[offset:offset+BytesPerHash], v)
	}

	return data
}

// Len returns the number of hash functions in the signature.
func (s *Signature) Len() int {
	return len(s.mins)
}

// fnvHash computes a 64-bit FNV-1a hash of the given data.
func fnvHash(data []byte) uint64 {
	h := fnv.New64a()
	_, _ = h.Write(data)

	return h.Sum64()
}

// mixHash combines a base hash with a seed using XOR and the splitmix64 finalizer.
func mixHash(base, seed uint64) uint64 {
	x := base ^ seed
	x = (x ^ (x >> mixShift1)) * mixMul1
	x = (x ^ (x >> mixShift2)) * mixMul2
	x ^= x >> mixShift3

	return x
}

// generateSeeds creates deterministic per-hash-function seeds using the
// splitmix64 sequence.
func generateSeeds(n int) []uint64 {
	seeds := make([]uint64, n)

	var state uint64 = baseSeed

	for i := range n {
		state = splitmix64(state)
		seeds[i] = state
	}

	return seeds
}

// splitmix64 advances the state and returns the next value in the sequence.
func splitmix64(state uint64) uint64 {
	state += 0x9e3779b97f4a7c15
	z := state
	z = (z ^ (z >> mixShift1)) * mixMul1
	z = (z ^ (z >> mixShift2)) * mixMul2
	z ^= z >> mixShift3

	return z
}
