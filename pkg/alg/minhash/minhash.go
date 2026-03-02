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
	"math"
	"sync"

	"github.com/Sumatoshi-tech/codefang/pkg/alg/internal/hashutil"
)

const (
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
		seeds: hashutil.GenerateSeeds(numHashes, hashutil.Splitmix64),
	}, nil
}

// Add updates all hash function minimums with the given token.
func (s *Signature) Add(token []byte) {
	baseHash := hashutil.FNV64a(token)

	s.mu.Lock()
	defer s.mu.Unlock()

	for i, seed := range s.seeds {
		h := hashutil.MixHash(baseHash, seed)
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
