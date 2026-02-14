package framework

import (
	"context"
	"maps"
	"sync"

	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

// DefaultBlobBatchArenaSize is the default size of the memory arena for blob loading (4MB).
const DefaultBlobBatchArenaSize = 4 * 1024 * 1024

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
	BlobCache      *GlobalBlobCache
	ArenaSize      int
}

// NewBlobPipeline creates a new blob pipeline.
func NewBlobPipeline(
	seqChan chan<- gitlib.WorkerRequest,
	poolChan chan<- gitlib.WorkerRequest,
	bufferSize int,
	workerCount int,
) *BlobPipeline {
	return NewBlobPipelineWithCache(seqChan, poolChan, bufferSize, workerCount, nil)
}

// NewBlobPipelineWithCache creates a new blob pipeline with an optional global blob cache.
func NewBlobPipelineWithCache(
	seqChan chan<- gitlib.WorkerRequest,
	poolChan chan<- gitlib.WorkerRequest,
	bufferSize int,
	workerCount int,
	cache *GlobalBlobCache,
) *BlobPipeline {
	if bufferSize <= 0 {
		bufferSize = 1
	}

	if workerCount <= 0 {
		workerCount = 1
	}

	return &BlobPipeline{
		SeqWorkerChan:  seqChan,
		PoolWorkerChan: poolChan,
		BufferSize:     bufferSize,
		WorkerCount:    workerCount,
		BlobCache:      cache,
		ArenaSize:      DefaultBlobBatchArenaSize,
	}
}

type batchBlobState struct {
	respChans []chan gitlib.BlobBatchResponse // Slice of response channels for sharded requests.
	results   map[gitlib.Hash]*gitlib.CachedBlob
	once      sync.Once
}

type blobJob struct {
	data       BlobData
	neededHash []gitlib.Hash                      // Hashes this job specifically needs.
	cacheHits  map[gitlib.Hash]*gitlib.CachedBlob // Blobs already found in global cache.
	batchState *batchBlobState                    // Shared state for the batch request.
}

// Process receives commit batches and outputs blob data.
func (p *BlobPipeline) Process(ctx context.Context, commits <-chan CommitBatch) <-chan BlobData {
	out := make(chan BlobData)
	jobs := make(chan blobJob, p.BufferSize)

	go p.runProducer(ctx, commits, jobs)
	go p.runConsumer(ctx, jobs, out)

	return out
}

// runProducer processes commit batches and creates blob load jobs.
func (p *BlobPipeline) runProducer(ctx context.Context, commits <-chan CommitBatch, jobs chan<- blobJob) {
	defer close(jobs)

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
		select {
		case p.PoolWorkerChan <- req:
		case <-ctx.Done():
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
	allNeededHashes := make(map[gitlib.Hash]bool)

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
			hashes := p.collectBlobHashes(resp.Changes)

			bJob.neededHash = hashes
			for _, h := range hashes {
				allNeededHashes[h] = true
			}
		}

		batchJobs[i] = bJob
		lastCommitHash = job.commit.Hash()
	}

	// Identify missing blobs across the entire batch.
	uniqueHashes := make([]gitlib.Hash, 0, len(allNeededHashes))
	for h := range allNeededHashes {
		uniqueHashes = append(uniqueHashes, h)
	}

	var (
		missingHashes   []gitlib.Hash
		globalCacheHits map[gitlib.Hash]*gitlib.CachedBlob
	)

	if p.BlobCache != nil && len(uniqueHashes) > 0 {
		globalCacheHits, missingHashes = p.BlobCache.GetMulti(uniqueHashes)
	} else {
		missingHashes = uniqueHashes
		globalCacheHits = make(map[gitlib.Hash]*gitlib.CachedBlob)
	}

	// Prepare shared batch state.
	batchState := &batchBlobState{
		results: make(map[gitlib.Hash]*gitlib.CachedBlob),
	}

	// Determine sharding.
	var chunkCount = 1
	if p.WorkerCount > 1 && len(missingHashes) > p.WorkerCount*2 { // Shard if enough items.
		chunkCount = p.WorkerCount
	}

	chunks := make([][]gitlib.Hash, chunkCount)
	for i, h := range missingHashes {
		idx := i % chunkCount
		chunks[idx] = append(chunks[idx], h)
	}

	// Fire batch requests.
	for _, chunk := range chunks {
		if len(chunk) == 0 {
			continue
		}

		// Allocate arena for this batch
		// We allocate one arena per request. It will be passed to CGO to fill.
		arena := make([]byte, p.ArenaSize)

		req := gitlib.BlobBatchRequest{
			Hashes: chunk,
			Arena:  arena,
		}
		respChan := make(chan gitlib.BlobBatchResponse, 1)
		req.Response = respChan
		batchState.respChans = append(batchState.respChans, respChan)

		select {
		case p.PoolWorkerChan <- req:
		case <-ctx.Done():
			return lastCommitHash
		}
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
func (p *BlobPipeline) runConsumer(ctx context.Context, jobs <-chan blobJob, out chan<- BlobData) {
	defer close(out)

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

// collectBlobResponse waits for and collects the blob response.
func (p *BlobPipeline) collectBlobResponse(ctx context.Context, job *blobJob) bool {
	// Initialize cache with hits we already have.
	cache := make(map[gitlib.Hash]*gitlib.CachedBlob)
	maps.Copy(cache, job.cacheHits)

	// If no batch request was needed, we are done.
	if job.batchState == nil || len(job.batchState.respChans) == 0 {
		job.data.BlobCache = cache

		return true
	}

	// Ensure batch request is processed exactly once.
	var success = true

	job.batchState.once.Do(func() {
		// New blobs to add to global cache.
		allNewBlobs := make(map[gitlib.Hash]*gitlib.CachedBlob)

		for _, ch := range job.batchState.respChans {
			select {
			case resp := <-ch:
				// So we can just use resp.Blobs.
				for _, blob := range resp.Blobs {
					if blob != nil {
						// We need the hash. CachedBlob has Hash() method?
						// Let's check CachedBlob definition.
						job.batchState.results[blob.Hash()] = blob
						allNewBlobs[blob.Hash()] = blob
					}
				}
			case <-ctx.Done():
				success = false

				return
			}
		}

		// Store new blobs in global cache.
		if p.BlobCache != nil && len(allNewBlobs) > 0 {
			p.BlobCache.PutMulti(allNewBlobs)
		}
	})

	if !success {
		return false
	}

	// Now grab from shared results what this job needs.
	for _, h := range job.neededHash {
		// If it wasn't in cacheHits, check shared results.
		if _, ok := cache[h]; !ok {
			if blob, found := job.batchState.results[h]; found {
				cache[h] = blob
			}
		}
	}

	job.data.BlobCache = cache

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
