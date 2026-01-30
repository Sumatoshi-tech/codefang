package gitlib

import (
	"context"
	"sync"
)

// BatchConfig configures batch processing parameters.
type BatchConfig struct {
	// BlobBatchSize is the number of blobs to load per batch.
	// Default: 100
	BlobBatchSize int

	// DiffBatchSize is the number of diffs to compute per batch.
	// Default: 50
	DiffBatchSize int

	// Workers is the number of parallel workers for processing.
	// Default: 1 (sequential processing within gitlib)
	Workers int
}

// DefaultBatchConfig returns the default batch configuration.
func DefaultBatchConfig() BatchConfig {
	return BatchConfig{
		BlobBatchSize: 100,
		DiffBatchSize: 50,
		Workers:       1,
	}
}

// BlobBatch represents a batch of loaded blobs.
type BlobBatch struct {
	// Blobs contains the loaded blob data.
	Blobs []*CachedBlob

	// Results contains detailed results for each blob request.
	Results []BlobResult

	// BatchID identifies this batch for ordering.
	BatchID int

	// Error is set if the entire batch failed.
	Error error
}

// DiffBatch represents a batch of computed diffs.
type DiffBatch struct {
	// Diffs contains the computed diff results.
	Diffs []DiffResult

	// Requests contains the original requests for correlation.
	Requests []DiffRequest

	// BatchID identifies this batch for ordering.
	BatchID int

	// Error is set if the entire batch failed.
	Error error
}

// BlobStreamer provides streaming blob loading with configurable batching.
type BlobStreamer struct {
	bridge *CGOBridge
	config BatchConfig
}

// NewBlobStreamer creates a new blob streamer for the repository.
func NewBlobStreamer(repo *Repository, config BatchConfig) *BlobStreamer {
	if config.BlobBatchSize <= 0 {
		config.BlobBatchSize = 100
	}
	return &BlobStreamer{
		bridge: NewCGOBridge(repo),
		config: config,
	}
}

// Stream reads hashes from the input channel, loads them in batches,
// and sends results to the output channel.
func (s *BlobStreamer) Stream(ctx context.Context, hashes <-chan []Hash) <-chan BlobBatch {
	out := make(chan BlobBatch)

	go func() {
		defer close(out)

		batchID := 0
		buffer := make([]Hash, 0, s.config.BlobBatchSize)

		flush := func() {
			if len(buffer) == 0 {
				return
			}

			// Load blobs via CGO bridge
			results := s.bridge.BatchLoadBlobs(buffer)

			// Convert to CachedBlobs
			blobs := make([]*CachedBlob, len(results))
			for i, r := range results {
				if r.Error == nil {
					blobs[i] = &CachedBlob{
						hash: r.Hash,
						size: r.Size,
						Data: r.Data,
					}
				}
			}

			batch := BlobBatch{
				Blobs:   blobs,
				Results: results,
				BatchID: batchID,
			}

			select {
			case out <- batch:
				batchID++
			case <-ctx.Done():
				return
			}

			buffer = buffer[:0]
		}

		for {
			select {
			case <-ctx.Done():
				return
			case hashBatch, ok := <-hashes:
				if !ok {
					// Channel closed, flush remaining
					flush()
					return
				}

				for _, h := range hashBatch {
					buffer = append(buffer, h)
					if len(buffer) >= s.config.BlobBatchSize {
						flush()
					}
				}
			}
		}
	}()

	return out
}

// DiffStreamer provides streaming diff computation with configurable batching.
type DiffStreamer struct {
	bridge *CGOBridge
	config BatchConfig
}

// NewDiffStreamer creates a new diff streamer for the repository.
func NewDiffStreamer(repo *Repository, config BatchConfig) *DiffStreamer {
	if config.DiffBatchSize <= 0 {
		config.DiffBatchSize = 50
	}
	return &DiffStreamer{
		bridge: NewCGOBridge(repo),
		config: config,
	}
}

// Stream reads diff requests from the input channel, computes them in batches,
// and sends results to the output channel.
func (s *DiffStreamer) Stream(ctx context.Context, requests <-chan []DiffRequest) <-chan DiffBatch {
	out := make(chan DiffBatch)

	go func() {
		defer close(out)

		batchID := 0
		buffer := make([]DiffRequest, 0, s.config.DiffBatchSize)

		flush := func() {
			if len(buffer) == 0 {
				return
			}

			// Compute diffs via CGO bridge
			results := s.bridge.BatchDiffBlobs(buffer)

			batch := DiffBatch{
				Diffs:    results,
				Requests: append([]DiffRequest{}, buffer...),
				BatchID:  batchID,
			}

			select {
			case out <- batch:
				batchID++
			case <-ctx.Done():
				return
			}

			buffer = buffer[:0]
		}

		for {
			select {
			case <-ctx.Done():
				return
			case reqBatch, ok := <-requests:
				if !ok {
					// Channel closed, flush remaining
					flush()
					return
				}

				for _, req := range reqBatch {
					buffer = append(buffer, req)
					if len(buffer) >= s.config.DiffBatchSize {
						flush()
					}
				}
			}
		}
	}()

	return out
}

// BatchProcessor provides a unified interface for batch processing
// both blobs and diffs in a coordinated manner.
type BatchProcessor struct {
	repo   *Repository
	config BatchConfig
	bridge *CGOBridge
	mu     sync.Mutex
}

// NewBatchProcessor creates a new batch processor for the repository.
func NewBatchProcessor(repo *Repository, config BatchConfig) *BatchProcessor {
	return &BatchProcessor{
		repo:   repo,
		config: config,
		bridge: NewCGOBridge(repo),
	}
}

// LoadBlobs loads a batch of blobs synchronously.
// Use this for simpler cases where streaming isn't needed.
func (p *BatchProcessor) LoadBlobs(hashes []Hash) []BlobResult {
	p.mu.Lock()
	defer p.mu.Unlock()
	return p.bridge.BatchLoadBlobs(hashes)
}

// ComputeDiffs computes a batch of diffs synchronously.
// Use this for simpler cases where streaming isn't needed.
func (p *BatchProcessor) ComputeDiffs(requests []DiffRequest) []DiffResult {
	p.mu.Lock()
	defer p.mu.Unlock()
	return p.bridge.BatchDiffBlobs(requests)
}

// LoadBlobsAsCached loads blobs and returns them as CachedBlobs.
// Failed loads return nil in the corresponding position.
func (p *BatchProcessor) LoadBlobsAsCached(hashes []Hash) []*CachedBlob {
	results := p.LoadBlobs(hashes)
	cached := make([]*CachedBlob, len(results))

	for i, r := range results {
		if r.Error == nil {
			cached[i] = &CachedBlob{
				hash: r.Hash,
				size: r.Size,
				Data: r.Data,
			}
		}
	}

	return cached
}

// ProcessCommitBlobs loads all blobs needed for a set of changes.
// Returns a map from hash to cached blob.
func (p *BatchProcessor) ProcessCommitBlobs(changes Changes) map[Hash]*CachedBlob {
	// Collect unique hashes
	hashSet := make(map[Hash]bool)
	for _, change := range changes {
		switch change.Action {
		case Insert:
			hashSet[change.To.Hash] = true
		case Delete:
			hashSet[change.From.Hash] = true
		case Modify:
			hashSet[change.From.Hash] = true
			hashSet[change.To.Hash] = true
		}
	}

	// Convert to slice
	hashes := make([]Hash, 0, len(hashSet))
	for h := range hashSet {
		hashes = append(hashes, h)
	}

	// Load all blobs
	cached := p.LoadBlobsAsCached(hashes)

	// Build result map
	result := make(map[Hash]*CachedBlob, len(hashes))
	for i, h := range hashes {
		if cached[i] != nil {
			result[h] = cached[i]
		}
	}

	return result
}

// ProcessCommitDiffs computes all diffs for modified files in a set of changes.
// Returns a map from file path to diff result.
func (p *BatchProcessor) ProcessCommitDiffs(changes Changes) map[string]DiffResult {
	// Collect diff requests for Modify actions
	var requests []DiffRequest
	var paths []string

	for _, change := range changes {
		if change.Action == Modify {
			requests = append(requests, DiffRequest{
				OldHash: change.From.Hash,
				NewHash: change.To.Hash,
				HasOld:  true,
				HasNew:  true,
			})
			paths = append(paths, change.To.Name)
		}
	}

	if len(requests) == 0 {
		return nil
	}

	// Compute diffs
	results := p.ComputeDiffs(requests)

	// Build result map
	resultMap := make(map[string]DiffResult, len(paths))
	for i, path := range paths {
		resultMap[path] = results[i]
	}

	return resultMap
}
