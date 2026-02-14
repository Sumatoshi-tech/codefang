package gitlib

import (
	"context"
	"sync"
)

// Default batch processing configuration values.
const (
	// defaultBlobBatchSize is the default number of blobs to load per batch.
	defaultBlobBatchSize = 100
	// defaultDiffBatchSize is the default number of diffs to compute per batch.
	defaultDiffBatchSize = 50
)

// BatchConfig configures batch processing parameters.
type BatchConfig struct {
	// BlobBatchSize is the number of blobs to load per batch.
	// Default: 100.
	BlobBatchSize int

	// DiffBatchSize is the number of diffs to compute per batch.
	// Default: 50.
	DiffBatchSize int

	// Workers is the number of parallel workers for processing.
	// Default: 1 (sequential processing within gitlib).
	Workers int
}

// DefaultBatchConfig returns the default batch configuration.
func DefaultBatchConfig() BatchConfig {
	return BatchConfig{
		BlobBatchSize: defaultBlobBatchSize,
		DiffBatchSize: defaultDiffBatchSize,
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
		config.BlobBatchSize = defaultBlobBatchSize
	}

	return &BlobStreamer{
		bridge: NewCGOBridge(repo),
		config: config,
	}
}

// blobStreamState holds the state for blob streaming.
type blobStreamState struct {
	streamer *BlobStreamer
	out      chan<- BlobBatch
	batchID  int
	buffer   []Hash
}

// flush sends the current buffer as a batch.
func (st *blobStreamState) flush(ctx context.Context) bool {
	if len(st.buffer) == 0 {
		return true
	}

	results := st.streamer.bridge.BatchLoadBlobs(st.buffer)
	blobs := make([]*CachedBlob, len(results))

	for i, r := range results {
		if r.Error == nil {
			blobs[i] = &CachedBlob{hash: r.Hash, size: r.Size, Data: r.Data}
		}
	}

	batch := BlobBatch{Blobs: blobs, Results: results, BatchID: st.batchID}

	select {
	case st.out <- batch:
		st.batchID++
	case <-ctx.Done():
		return false
	}

	st.buffer = st.buffer[:0]

	return true
}

// processHashes adds hashes to the buffer, flushing when full.
func (st *blobStreamState) processHashes(ctx context.Context, hashBatch []Hash) bool {
	for _, h := range hashBatch {
		st.buffer = append(st.buffer, h)

		if len(st.buffer) >= st.streamer.config.BlobBatchSize {
			if !st.flush(ctx) {
				return false
			}
		}
	}

	return true
}

// Stream reads hashes from the input channel, loads them in batches,
// and sends results to the output channel.
func (s *BlobStreamer) Stream(ctx context.Context, hashes <-chan []Hash) <-chan BlobBatch {
	out := make(chan BlobBatch)

	go func() {
		defer close(out)

		st := &blobStreamState{
			streamer: s,
			out:      out,
			buffer:   make([]Hash, 0, s.config.BlobBatchSize),
		}

		for {
			select {
			case <-ctx.Done():
				return
			case hashBatch, ok := <-hashes:
				if !ok {
					st.flush(ctx)

					return
				}

				if !st.processHashes(ctx, hashBatch) {
					return
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
		config.DiffBatchSize = defaultDiffBatchSize
	}

	return &DiffStreamer{
		bridge: NewCGOBridge(repo),
		config: config,
	}
}

// diffStreamState holds the state for diff streaming.
type diffStreamState struct {
	streamer *DiffStreamer
	out      chan<- DiffBatch
	batchID  int
	buffer   []DiffRequest
}

// flush sends the current buffer as a batch.
func (st *diffStreamState) flush(ctx context.Context) bool {
	if len(st.buffer) == 0 {
		return true
	}

	results := st.streamer.bridge.BatchDiffBlobs(st.buffer)
	batch := DiffBatch{
		Diffs:    results,
		Requests: append([]DiffRequest{}, st.buffer...),
		BatchID:  st.batchID,
	}

	select {
	case st.out <- batch:
		st.batchID++
	case <-ctx.Done():
		return false
	}

	st.buffer = st.buffer[:0]

	return true
}

// processRequests adds requests to the buffer, flushing when full.
func (st *diffStreamState) processRequests(ctx context.Context, reqBatch []DiffRequest) bool {
	for _, req := range reqBatch {
		st.buffer = append(st.buffer, req)

		if len(st.buffer) >= st.streamer.config.DiffBatchSize {
			if !st.flush(ctx) {
				return false
			}
		}
	}

	return true
}

// Stream reads diff requests from the input channel, computes them in batches,
// and sends results to the output channel.
func (s *DiffStreamer) Stream(ctx context.Context, requests <-chan []DiffRequest) <-chan DiffBatch {
	out := make(chan DiffBatch)

	go s.runDiffStream(ctx, requests, out)

	return out
}

// runDiffStream is the main goroutine for processing diff requests.
func (s *DiffStreamer) runDiffStream(ctx context.Context, requests <-chan []DiffRequest, out chan<- DiffBatch) {
	defer close(out)

	st := &diffStreamState{
		streamer: s,
		out:      out,
		buffer:   make([]DiffRequest, 0, s.config.DiffBatchSize),
	}

	for {
		select {
		case <-ctx.Done():
			return
		case reqBatch, ok := <-requests:
			if !ok {
				st.flush(ctx)

				return
			}

			if !st.processRequests(ctx, reqBatch) {
				return
			}
		}
	}
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
	// Collect unique hashes.
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

	// Convert to slice.
	hashes := make([]Hash, 0, len(hashSet))

	for h := range hashSet {
		hashes = append(hashes, h)
	}

	// Load all blobs.
	cached := p.LoadBlobsAsCached(hashes)

	// Build result map.
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
	// Collect diff requests for Modify actions.
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

	// Compute diffs.
	results := p.ComputeDiffs(requests)

	// Build result map.
	resultMap := make(map[string]DiffResult, len(paths))

	for i, path := range paths {
		resultMap[path] = results[i]
	}

	return resultMap
}
