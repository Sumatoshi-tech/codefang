package framework

import (
	"context"
	"errors"
	"fmt"
	"log"
	"maps"
	"sync"

	"github.com/Sumatoshi-tech/codefang/internal/cache"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
	"github.com/Sumatoshi-tech/codefang/pkg/pipeline"
)

// DefaultBlobBatchArenaSize is the default size of the memory arena for blob loading (4MB).
const DefaultBlobBatchArenaSize = 4 * 1024 * 1024

// estimatedHashesPerCommit is the average number of blob hashes per commit,
// used to pre-size the allNeededHashes map.
const estimatedHashesPerCommit = 4

// maxChangesPerCommit caps the number of file changes per commit that will be
// processed. Commits exceeding this (vendor moves, mass renames, generated code
// imports) are skipped — their Error field is set to ErrCommitTooLarge so all
// downstream stages (diff, UAST, analyzers) skip them cleanly. This bounds peak
// Go heap usage regardless of commit size.
const maxChangesPerCommit = 2000

// estimatedHashBytes is the approximate memory per hash entry for metrics.
const estimatedHashBytes = 256

// ErrCommitTooLarge is set on BlobData.Error for commits exceeding maxChangesPerCommit.
// The runner uses [errors.Is] to distinguish this from fatal pipeline errors and
// skips the commit instead of aborting.
var ErrCommitTooLarge = errors.New("commit exceeds max changes cap")

// BlobData holds loaded blob data for a commit.
type BlobData struct {
	Commit    *gitlib.Commit
	Index     int
	Changes   gitlib.Changes
	BlobCache map[gitlib.Hash]*gitlib.CachedBlob
	Error     error
}

// BlobPipeline processes commit batches to load blobs.
type BlobPipeline struct {
	SeqWorkerChan  chan<- gitlib.WorkerRequest
	PoolWorkerChan chan<- gitlib.WorkerRequest
	BufferSize     int
	WorkerCount    int
	BlobCache      *cache.LRUBlobCache
	ArenaSize      int

	// Metrics provides per-stage counters for memory triage. Nil-safe.
	Metrics *StageMetrics

	// arenaPool recycles arena byte slices to avoid repeated large allocations.
	arenaPool sync.Pool

	// dispatch sends a worker request to the pool. Initialized from PoolWorkerChan
	// in the constructor; can be overridden for testing.
	dispatch pipeline.DispatchFunc[gitlib.WorkerRequest]

	// blobFetch checks the global blob cache for previously loaded blobs.
	// Returns hits and misses from the cache lookup.
	blobFetch pipeline.Fetcher[[]gitlib.Hash, blobFetchResult]
}

// blobFetchResult holds the outcome of a blob cache lookup.
type blobFetchResult struct {
	hits   map[gitlib.Hash]*gitlib.CachedBlob
	misses []gitlib.Hash
}

// NewBlobPipelineWithCache creates a new blob pipeline with an optional global blob cache.
func NewBlobPipelineWithCache(
	seqChan chan<- gitlib.WorkerRequest,
	poolChan chan<- gitlib.WorkerRequest,
	bufferSize int,
	workerCount int,
	blobCache *cache.LRUBlobCache,
) *BlobPipeline {
	if bufferSize <= 0 {
		bufferSize = 1
	}

	if workerCount <= 0 {
		workerCount = 1
	}

	p := &BlobPipeline{
		SeqWorkerChan:  seqChan,
		PoolWorkerChan: poolChan,
		BufferSize:     bufferSize,
		WorkerCount:    workerCount,
		BlobCache:      blobCache,
		ArenaSize:      DefaultBlobBatchArenaSize,
	}

	p.arenaPool = sync.Pool{New: func() any {
		return make([]byte, p.ArenaSize)
	}}

	p.dispatch = pipeline.DispatchFunc[gitlib.WorkerRequest](func(ctx context.Context, req gitlib.WorkerRequest) error {
		select {
		case poolChan <- req:
			return nil
		case <-ctx.Done():
			return ctx.Err()
		}
	})

	type blobFetcher = pipeline.FetcherFunc[[]gitlib.Hash, blobFetchResult]

	if blobCache != nil {
		p.blobFetch = blobFetcher(func(_ context.Context, hashes []gitlib.Hash) (blobFetchResult, error) {
			hits, misses := blobCache.GetMulti(hashes)

			return blobFetchResult{hits: hits, misses: misses}, nil
		})
	} else {
		p.blobFetch = blobFetcher(func(_ context.Context, hashes []gitlib.Hash) (blobFetchResult, error) {
			return blobFetchResult{
				hits:   make(map[gitlib.Hash]*gitlib.CachedBlob),
				misses: hashes,
			}, nil
		})
	}

	return p
}

type blobJob struct {
	data       BlobData
	neededHash []gitlib.Hash                      // Hashes this job specifically needs.
	cacheHits  map[gitlib.Hash]*gitlib.CachedBlob // Blobs already found in global cache.

	// Shared state for the batch request.
	batchState *pipeline.SharedResponse[map[gitlib.Hash]*gitlib.CachedBlob]
}

// Process receives commit batches and outputs blob data.
func (p *BlobPipeline) Process(ctx context.Context, commits <-chan CommitBatch) <-chan BlobData {
	pc := pipeline.RunPC[<-chan CommitBatch, BlobData, blobJob]{
		Buffer:  p.BufferSize,
		Produce: p.runProducer,
		Consume: p.runConsumer,
	}

	return pc.Run(ctx, commits)
}

// runProducer processes commit batches and creates blob load jobs.
// Channel lifecycle is managed by RunPC; this function must not close jobs.
func (p *BlobPipeline) runProducer(ctx context.Context, commits <-chan CommitBatch, jobs chan<- blobJob) {
	var previousCommitHash gitlib.Hash

	for batch := range commits {
		select {
		case <-ctx.Done():
			return
		default:
		}

		previousCommitHash = p.processBatch(ctx, batch, previousCommitHash, jobs)
		if ctx.Err() != nil {
			return
		}
	}
}

// processBatch processes a single commit batch and returns the last commit hash.
func (p *BlobPipeline) processBatch(
	ctx context.Context, batch CommitBatch, previousHash gitlib.Hash, jobs chan<- blobJob,
) gitlib.Hash {
	// First pass: Dispatch all tree diffs in parallel to the worker pool.
	type treeDiffJob struct {
		index    int
		commit   *gitlib.Commit
		respChan chan gitlib.TreeDiffResponse
	}

	diffJobs := make([]treeDiffJob, len(batch.Commits))

	for i, commit := range batch.Commits {
		respChan := make(chan gitlib.TreeDiffResponse, 1)

		// With first-parent walk, previous in stream equals parent; diff base must match burndown state.
		var prevHash gitlib.Hash

		switch {
		case commit.NumParents() > 0:
			prevHash = commit.ParentHash(0)
		case i > 0:
			prevHash = batch.Commits[i-1].Hash()
		default:
			prevHash = previousHash
		}

		req := gitlib.TreeDiffRequest{
			PreviousCommitHash: prevHash,
			CommitHash:         commit.Hash(),
			Response:           respChan,
		}

		// Send to POOL workers for parallelism.
		dispatchErr := p.dispatch(ctx, gitlib.WithContext(ctx, req))
		if dispatchErr != nil {
			return gitlib.Hash{}
		}

		diffJobs[i] = treeDiffJob{
			index:    i,
			commit:   commit,
			respChan: respChan,
		}
	}

	// Collect Tree Diffs.
	batchJobs := make([]blobJob, len(batch.Commits))
	allNeededHashes := make(map[gitlib.Hash]bool, len(batch.Commits)*estimatedHashesPerCommit)

	var lastCommitHash gitlib.Hash

	for i, job := range diffJobs {
		resp := <-job.respChan

		// Helper to free tree if we don't need it (we don't pass it forward anymore).
		if resp.CurrentTree != nil {
			resp.CurrentTree.Free()
		}

		bJob := blobJob{
			data: BlobData{
				Commit:  job.commit,
				Index:   batch.StartIndex + job.index,
				Changes: resp.Changes,
				Error:   resp.Error,
			},
		}

		if resp.Error == nil {
			// Skip monster commits (vendor moves, mass renames) by setting
			// ErrCommitTooLarge. The runner detects this and skips the commit
			// instead of aborting the pipeline.
			if len(resp.Changes) > maxChangesPerCommit {
				log.Printf("blob pipeline: skipping commit %s (%d changes > %d cap)",
					job.commit.Hash(), len(resp.Changes), maxChangesPerCommit)
				bJob.data.Changes = nil
				bJob.data.Error = fmt.Errorf("%w: %s has %d changes",
					ErrCommitTooLarge, job.commit.Hash(), len(resp.Changes))
			} else {
				hashes := p.collectBlobHashes(resp.Changes)

				bJob.neededHash = hashes
				for _, h := range hashes {
					allNeededHashes[h] = true
				}
			}
		}

		batchJobs[i] = bJob
		lastCommitHash = job.commit.Hash()
	}

	// Record per-batch change count for memory triage.
	if p.Metrics != nil {
		totalChanges := int64(0)
		for _, job := range batchJobs {
			totalChanges += int64(len(job.data.Changes))
		}

		p.Metrics.RecordBlobBatch(totalChanges, int64(len(allNeededHashes))*estimatedHashBytes) // Estimated bytes per hash.
	}

	// Identify missing blobs across the entire batch.
	uniqueHashes := make([]gitlib.Hash, 0, len(allNeededHashes))
	for h := range allNeededHashes {
		uniqueHashes = append(uniqueHashes, h)
	}

	// Check the blob cache via the Fetcher.
	// The blob fetcher never returns an error (cache lookup is infallible).
	cacheResult, fetchErr := p.blobFetch.Fetch(ctx, uniqueHashes)
	if fetchErr != nil {
		return lastCommitHash
	}

	globalCacheHits := cacheResult.hits
	missingHashes := cacheResult.misses

	// Fire sharded blob requests and create shared response.
	batchState, earlyReturn := p.fireBlobBatchRequests(ctx, missingHashes)
	if earlyReturn {
		return lastCommitHash
	}

	// Second pass: Dispatch jobs.
	for i := range batchJobs {
		job := batchJobs[i]

		// Assign cache hits relevant to this job.
		job.cacheHits = make(map[gitlib.Hash]*gitlib.CachedBlob)
		for _, h := range job.neededHash {
			if blob, ok := globalCacheHits[h]; ok {
				job.cacheHits[h] = blob
			}
		}

		job.batchState = batchState

		select {
		case jobs <- job:
		case <-ctx.Done():
			return lastCommitHash
		}
	}

	return lastCommitHash
}

// runConsumer waits for blob responses and outputs blob data.
// Channel lifecycle is managed by RunPC; this function must not close out.
func (p *BlobPipeline) runConsumer(ctx context.Context, jobs <-chan blobJob, out chan<- BlobData) {
	for job := range jobs {
		select {
		case <-ctx.Done():
			return
		default:
		}

		if job.data.Error != nil {
			out <- job.data

			continue
		}

		if !p.collectBlobResponse(ctx, &job) {
			return
		}

		select {
		case out <- job.data:
		case <-ctx.Done():
			return
		}
	}
}

// fireBlobBatchRequests shards missing hashes across workers and returns a
// [pipeline.SharedResponse] that merges all sharded blob responses. Returns (nil, false)
// when there are no missing hashes. The bool indicates early return due to
// context cancellation.
func (p *BlobPipeline) fireBlobBatchRequests(
	ctx context.Context, missingHashes []gitlib.Hash,
) (*pipeline.SharedResponse[map[gitlib.Hash]*gitlib.CachedBlob], bool) {
	if len(missingHashes) == 0 {
		return nil, false
	}

	// Determine sharding.
	chunkCount := 1
	if p.WorkerCount > 1 && len(missingHashes) > p.WorkerCount*2 { // Shard if enough items.
		chunkCount = p.WorkerCount
	}

	chunks := make([][]gitlib.Hash, chunkCount)
	for i, h := range missingHashes {
		idx := i % chunkCount
		chunks[idx] = append(chunks[idx], h)
	}

	// Fire batch requests and collect response channels.
	// Track arenas so we can return them to the pool after cloning.
	var (
		respChans []chan gitlib.BlobBatchResponse
		arenas    [][]byte
	)

	for _, chunk := range chunks {
		if len(chunk) == 0 {
			continue
		}

		// Get arena from pool instead of allocating fresh.
		arena, ok := p.arenaPool.Get().([]byte)
		if !ok {
			arena = make([]byte, p.ArenaSize)
		}

		arenas = append(arenas, arena)

		req := gitlib.BlobBatchRequest{
			Hashes: chunk,
			Arena:  arena,
		}

		respChan := make(chan gitlib.BlobBatchResponse, 1)
		req.Response = respChan
		respChans = append(respChans, respChan)

		dispatchErr := p.dispatch(ctx, gitlib.WithContext(ctx, req))
		if dispatchErr != nil {
			// Return arenas on error path.
			for _, a := range arenas {
				p.arenaPool.Put(a) //nolint:staticcheck // sync.Pool accepts []byte
			}

			return nil, true
		}
	}

	// Create a shared response that merges all sharded blob responses.
	blobCache := p.BlobCache
	arenaPool := &p.arenaPool

	return pipeline.NewSharedResponse(func(ctx context.Context) (map[gitlib.Hash]*gitlib.CachedBlob, error) {
		results := make(map[gitlib.Hash]*gitlib.CachedBlob, len(missingHashes))
		allNewBlobs := make(map[gitlib.Hash]*gitlib.CachedBlob, len(missingHashes))

		for _, ch := range respChans {
			select {
			case resp := <-ch:
				for _, blob := range resp.Blobs {
					if blob != nil {
						// Clone blob data to detach from arena memory.
						// This allows the arena to be recycled.
						cloned := blob.Clone()
						results[cloned.Hash()] = cloned
						allNewBlobs[cloned.Hash()] = cloned
					}
				}
			case <-ctx.Done():
				for _, a := range arenas {
					arenaPool.Put(a) //nolint:staticcheck // sync.Pool accepts []byte
				}

				return nil, ctx.Err()
			}
		}

		// Return arenas to pool now that all blobs are cloned.
		for _, a := range arenas {
			arenaPool.Put(a) //nolint:staticcheck // sync.Pool accepts []byte
		}

		// Store cloned blobs directly (skip re-cloning since we own them).
		if blobCache != nil && len(allNewBlobs) > 0 {
			blobCache.PutMultiOwned(allNewBlobs)
		}

		return results, nil
	}), false
}

// collectBlobResponse waits for and collects the blob response.
func (p *BlobPipeline) collectBlobResponse(ctx context.Context, job *blobJob) bool {
	// Initialize collected blobs with hits we already have.
	blobs := make(map[gitlib.Hash]*gitlib.CachedBlob)
	maps.Copy(blobs, job.cacheHits)

	// If no batch request was needed, we are done.
	if job.batchState == nil {
		job.data.BlobCache = blobs

		return true
	}

	// Ensure batch request is processed exactly once.
	results, err := job.batchState.Get(ctx)
	if err != nil {
		return false
	}

	// Now grab from shared results what this job needs.
	for _, h := range job.neededHash {
		// If it wasn't in cacheHits, check shared results.
		if _, ok := blobs[h]; !ok {
			if blob, found := results[h]; found {
				blobs[h] = blob
			}
		}
	}

	job.data.BlobCache = blobs

	return true
}

// File mode constants for git tree entries.
const (
	FileModeCommit = 0o160000
	FileModeTree   = 0o040000
	FileModeBlob   = 0o100644
	FileModeExec   = 0o100755
	FileModeLink   = 0o120000
)

func (p *BlobPipeline) collectBlobHashes(changes gitlib.Changes) []gitlib.Hash {
	hashSet := make(map[gitlib.Hash]bool)

	for _, change := range changes {
		switch change.Action {
		case gitlib.Insert:
			hashSet[change.To.Hash] = true
		case gitlib.Delete:
			hashSet[change.From.Hash] = true
		case gitlib.Modify:
			hashSet[change.From.Hash] = true
			hashSet[change.To.Hash] = true
		}
	}

	hashes := make([]gitlib.Hash, 0, len(hashSet))

	for h := range hashSet {
		hashes = append(hashes, h)
	}

	return hashes
}
