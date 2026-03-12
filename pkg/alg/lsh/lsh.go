// Package lsh provides a Locality-Sensitive Hashing index for fast
// approximate nearest-neighbor retrieval of MinHash signatures.
//
// LSH groups similar MinHash signatures into the same buckets by hashing
// bands of consecutive hash values. This enables O(N) indexing and sublinear
// query time, replacing O(N^2) pairwise comparison.
//
// The index is parameterized by numBands and numRows where
// numBands * numRows = numHashes. Higher numBands lowers the similarity
// threshold for candidate retrieval.
package lsh

import (
	"encoding/binary"
	"errors"
	"hash/fnv"
	"sync"

	"github.com/Sumatoshi-tech/codefang/pkg/alg/minhash"
)

const (
	// bytesPerUint64 is the size of a uint64 in bytes for band hashing.
	bytesPerUint64 = 8
)

var (
	// ErrInvalidParams is returned when numBands or numRows is not positive.
	ErrInvalidParams = errors.New("lsh: numBands and numRows must be positive")

	// ErrNilSignature is returned when a nil signature is provided.
	ErrNilSignature = errors.New("lsh: signature must not be nil")

	// ErrSizeMismatch is returned when signature size does not match numBands * numRows.
	ErrSizeMismatch = errors.New("lsh: signature size must equal numBands * numRows")
)

// Index is a thread-safe LSH index for approximate nearest-neighbor retrieval.
type Index struct {
	mu       sync.RWMutex
	numBands int
	numRows  int
	bands    []map[uint64]map[string]bool
	sigs     map[string]*minhash.Signature
}

// New creates a new LSH index with the given number of bands and rows per band.
// The total number of hash functions expected from signatures is numBands * numRows.
func New(numBands, numRows int) (*Index, error) {
	if numBands <= 0 || numRows <= 0 {
		return nil, ErrInvalidParams
	}

	bands := make([]map[uint64]map[string]bool, numBands)
	for i := range bands {
		bands[i] = make(map[uint64]map[string]bool)
	}

	return &Index{
		numBands: numBands,
		numRows:  numRows,
		bands:    bands,
		sigs:     make(map[string]*minhash.Signature),
	}, nil
}

// Insert adds a signature to the index with the given identifier.
// Returns an error if sig is nil or its size does not match numBands * numRows.
func (idx *Index) Insert(id string, sig *minhash.Signature) error {
	if sig == nil {
		return ErrNilSignature
	}

	expectedSize := idx.numBands * idx.numRows
	if sig.Len() != expectedSize {
		return ErrSizeMismatch
	}

	bandHashes := idx.computeBandHashes(sig)

	idx.mu.Lock()
	defer idx.mu.Unlock()

	// Remove old entry if ID already exists.
	if oldSig, exists := idx.sigs[id]; exists {
		idx.removeLocked(id, oldSig)
	}

	idx.sigs[id] = sig

	for b, h := range bandHashes {
		bucket := idx.bands[b][h]
		if bucket == nil {
			bucket = make(map[string]bool)
			idx.bands[b][h] = bucket
		}

		bucket[id] = true
	}

	return nil
}

// Query returns deduplicated candidate IDs whose signatures share at least
// one band hash with the query signature.
func (idx *Index) Query(sig *minhash.Signature) ([]string, error) {
	if sig == nil {
		return nil, ErrNilSignature
	}

	expectedSize := idx.numBands * idx.numRows
	if sig.Len() != expectedSize {
		return nil, ErrSizeMismatch
	}

	bandHashes := idx.computeBandHashes(sig)

	idx.mu.RLock()
	defer idx.mu.RUnlock()

	seen := make(map[string]bool)

	for b, h := range bandHashes {
		bucket := idx.bands[b][h]
		for id := range bucket {
			seen[id] = true
		}
	}

	result := make([]string, 0, len(seen))
	for id := range seen {
		result = append(result, id)
	}

	return result, nil
}

// QueryThreshold returns candidate IDs whose exact MinHash similarity with
// the query signature is at or above the given threshold.
func (idx *Index) QueryThreshold(sig *minhash.Signature, threshold float64) ([]string, error) {
	candidates, err := idx.Query(sig)
	if err != nil {
		return nil, err
	}

	idx.mu.RLock()
	defer idx.mu.RUnlock()

	result := make([]string, 0)

	for _, id := range candidates {
		stored := idx.sigs[id]
		if stored == nil {
			continue
		}

		sim, simErr := sig.Similarity(stored)
		if simErr != nil {
			continue
		}

		if sim >= threshold {
			result = append(result, id)
		}
	}

	return result, nil
}

// removeLocked removes a signature from all band buckets. Must be called with mu held.
func (idx *Index) removeLocked(id string, sig *minhash.Signature) {
	bandHashes := idx.computeBandHashes(sig)

	for b, h := range bandHashes {
		bucket := idx.bands[b][h]
		delete(bucket, id)

		if len(bucket) == 0 {
			delete(idx.bands[b], h)
		}
	}

	delete(idx.sigs, id)
}

// computeBandHashes computes the FNV-1a hash for each band of the signature.
func (idx *Index) computeBandHashes(sig *minhash.Signature) []uint64 {
	data := sig.Bytes()

	// Skip the 4-byte header.
	hashData := data[minhash.HeaderSize:]
	hashes := make([]uint64, idx.numBands)

	buf := make([]byte, bytesPerUint64)

	for b := range idx.numBands {
		h := fnv.New64a()

		// Write band index for domain separation.
		binary.BigEndian.PutUint64(buf, uint64(b))
		_, _ = h.Write(buf)

		// Write the rows for this band.
		start := b * idx.numRows * bytesPerUint64
		end := start + idx.numRows*bytesPerUint64
		_, _ = h.Write(hashData[start:end])

		hashes[b] = h.Sum64()
	}

	return hashes
}
