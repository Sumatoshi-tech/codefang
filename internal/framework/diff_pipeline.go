package framework

import (
	"context"
	"errors"
	"maps"
	"strings"

	"github.com/sergi/go-diff/diffmatchpatch"

	"github.com/Sumatoshi-tech/codefang/internal/plumbing"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
	"github.com/Sumatoshi-tech/codefang/pkg/pipeline"
	"github.com/Sumatoshi-tech/codefang/pkg/uast"
)

// ErrCacheMiss is returned by a cache-backed Fetcher when the key is not found.
var ErrCacheMiss = errors.New("cache miss")

// CommitData holds all processed data for a commit.
type CommitData struct {
	Commit        *gitlib.Commit
	Index         int
	Changes       gitlib.Changes
	BlobCache     map[gitlib.Hash]*gitlib.CachedBlob
	FileDiffs     map[string]plumbing.FileDiffData
	UASTChanges   []uast.Change // Pre-computed UAST changes (nil if not computed).
	UASTSpillPath string        // Path to spilled UAST gob file (empty if in-memory).
	Error         error
}

// DiffPipeline processes blob data to compute file diffs.
type DiffPipeline struct {
	PoolWorkerChan chan<- gitlib.WorkerRequest
	BufferSize     int
	DiffCache      *DiffCache

	// NoBatch disables cross-commit batching. Each diff request fires immediately.
	// Useful for debugging or single-commit analysis.
	NoBatch bool

	// dispatch sends a worker request to the pool. Initialized from PoolWorkerChan
	// in the constructor; can be overridden for testing.
	dispatch pipeline.DispatchFunc[gitlib.WorkerRequest]

	// diffFetch checks the cache for a previously computed diff.
	// Returns ErrCacheMiss when the key is not found.
	diffFetch pipeline.Fetcher[DiffKey, plumbing.FileDiffData]

	// diffStore writes a computed diff result to the cache.
	diffStore func(DiffKey, plumbing.FileDiffData)
}

// NewDiffPipelineWithCache creates a new diff pipeline with an optional diff cache.
func NewDiffPipelineWithCache(workerChan chan<- gitlib.WorkerRequest, bufferSize int, cache *DiffCache) *DiffPipeline {
	if bufferSize <= 0 {
		bufferSize = 1
	}

	p := &DiffPipeline{
		PoolWorkerChan: workerChan,
		BufferSize:     bufferSize,
		DiffCache:      cache,
	}

	p.dispatch = pipeline.DispatchFunc[gitlib.WorkerRequest](func(ctx context.Context, req gitlib.WorkerRequest) error {
		select {
		case workerChan <- req:
			return nil
		case <-ctx.Done():
			return ctx.Err()
		}
	})

	type diffFetcher = pipeline.FetcherFunc[DiffKey, plumbing.FileDiffData]

	if cache != nil {
		p.diffFetch = diffFetcher(func(_ context.Context, key DiffKey) (plumbing.FileDiffData, error) {
			if cached, found := cache.Get(key); found {
				return cached, nil
			}

			return plumbing.FileDiffData{}, ErrCacheMiss
		})
		p.diffStore = func(key DiffKey, val plumbing.FileDiffData) {
			cache.Put(key, val)
		}
	} else {
		p.diffFetch = diffFetcher(func(_ context.Context, _ DiffKey) (plumbing.FileDiffData, error) {
			return plumbing.FileDiffData{}, ErrCacheMiss
		})
		p.diffStore = func(_ DiffKey, _ plumbing.FileDiffData) {}
	}

	return p
}

type diffJob struct {
	data      CommitData
	paths     []string                         // paths for diffs requested from C.
	changes   []*gitlib.Change                 // changes for diffs requested from C.
	cacheHits map[string]plumbing.FileDiffData // path -> cached diff.

	// Batching fields for cross-commit batching.
	pendingRequests []gitlib.DiffRequest
	batchResp       *pipeline.SharedResponse[[]gitlib.DiffResult]
	batchOffset     int
	batchLen        int
}

// Process receives blob data and outputs commit data with computed diffs.
func (p *DiffPipeline) Process(ctx context.Context, blobs <-chan BlobData) <-chan CommitData {
	// diffJobBufferMultiplier scales the job buffer relative to pipeline buffer size.
	// A larger buffer allows accumulating more diff jobs for cross-commit batching.
	const diffJobBufferMultiplier = 10

	pc := pipeline.RunPC[<-chan BlobData, CommitData, diffJob]{
		Buffer:  p.BufferSize * diffJobBufferMultiplier,
		Produce: p.runDiffProducer,
		Consume: p.runDiffConsumer,
	}

	return pc.Run(ctx, blobs)
}

// runDiffProducer processes blob data and creates diff jobs.
// Channel lifecycle is managed by RunPC; this function must not close jobs.
func (p *DiffPipeline) runDiffProducer(ctx context.Context, blobs <-chan BlobData, jobs chan<- diffJob) {
	// We accumulate diff requests until we have a decent batch size (e.g. 1000 diffs)
	// or until input channel is dry.
	// Since BlobPipeline emits BlobData which already contains multiple diffs per commit,
	// we are effectively re-batching across commits.
	const maxBatchSize = 1000

	var batcher pipeline.Batcher[gitlib.DiffRequest, []gitlib.DiffRequest]
	if p.NoBatch {
		batcher = &pipeline.PassthroughBatcher[gitlib.DiffRequest]{}
	} else {
		batcher = pipeline.NewThresholdBatcher[gitlib.DiffRequest](maxBatchSize)
	}

	var pendingJobs []*diffJob

	flush := func() {
		if len(pendingJobs) == 0 {
			return
		}

		var sharedResp *pipeline.SharedResponse[[]gitlib.DiffResult]

		// Drain accumulated requests from the batcher.
		if batchReqs, ok := batcher.Flush(); ok {
			req := gitlib.DiffBatchRequest{Requests: batchReqs}
			respChan := make(chan gitlib.DiffBatchResponse, 1)
			req.Response = respChan

			// Send request.
			dispatchErr := p.dispatch(ctx, gitlib.WithContext(ctx, req))
			if dispatchErr != nil {
				return
			}

			// Create a shared response for this batch.
			sharedResp = pipeline.NewSharedResponse(func(ctx context.Context) ([]gitlib.DiffResult, error) {
				select {
				case resp := <-respChan:
					return resp.Results, nil
				case <-ctx.Done():
					return nil, ctx.Err()
				}
			})
		}

		// Assign shared response to all jobs and dispatch.
		startIdx := 0

		for _, job := range pendingJobs {
			count := len(job.pendingRequests)
			if count > 0 && sharedResp != nil {
				job.batchResp = sharedResp
				job.batchOffset = startIdx
				job.batchLen = count
				startIdx += count
			}

			select {
			case jobs <- *job:
			case <-ctx.Done():
				return
			}
		}

		pendingJobs = nil
	}

	for blobData := range blobs {
		select {
		case <-ctx.Done():
			return
		default:
		}

		job, reqs := p.createDiffJobInternal(ctx, blobData)
		if job == nil {
			return
		}

		ready := false

		if len(reqs) > 0 {
			job.pendingRequests = reqs

			for _, req := range reqs {
				if batcher.Add(req) {
					ready = true
				}
			}
		}

		pendingJobs = append(pendingJobs, job)

		if ready {
			flush()
		}
	}

	// Flush remaining.
	flush()
}

// createDiffJobInternal prepares the job but doesn't fire requests.
func (p *DiffPipeline) createDiffJobInternal(ctx context.Context, blobData BlobData) (*diffJob, []gitlib.DiffRequest) {
	commitData := CommitData{
		Commit:    blobData.Commit,
		Index:     blobData.Index,
		Changes:   blobData.Changes,
		BlobCache: blobData.BlobCache,
		Error:     blobData.Error,
	}

	job := &diffJob{data: commitData}

	if commitData.Error != nil {
		return job, nil
	}

	req, paths, changes, cacheHits := p.prepareDiffRequest(ctx, blobData)
	job.paths = paths
	job.changes = changes
	job.cacheHits = cacheHits

	return job, req.Requests
}

// runDiffConsumer waits for diff responses and outputs commit data.
// Channel lifecycle is managed by RunPC; this function must not close out.
func (p *DiffPipeline) runDiffConsumer(ctx context.Context, jobs <-chan diffJob, out chan<- CommitData) {
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

		job.data.FileDiffs = make(map[string]plumbing.FileDiffData)

		// Add cache hits first.
		if len(job.cacheHits) > 0 {
			maps.Copy(job.data.FileDiffs, job.cacheHits)
		}

		// Process batched diff response.
		if job.batchResp != nil && job.batchLen > 0 {
			batchResults, err := job.batchResp.Get(ctx)
			if err != nil {
				job.data.Error = err
			} else if job.batchOffset+job.batchLen <= len(batchResults) {
				// Extract this job's portion of results.
				jobResults := batchResults[job.batchOffset : job.batchOffset+job.batchLen]
				resp := gitlib.DiffBatchResponse{Results: jobResults}
				p.processDiffResponse(job.data, resp, job.paths, job.changes)
			}
		}

		select {
		case out <- job.data:
		case <-ctx.Done():
			return
		}
	}
}

func (p *DiffPipeline) prepareDiffRequest(ctx context.Context, blobData BlobData) (
	req gitlib.DiffBatchRequest,
	paths []string,
	changes []*gitlib.Change,
	cacheHits map[string]plumbing.FileDiffData,
) {
	var requests []gitlib.DiffRequest

	for _, change := range blobData.Changes {
		if change.Action != gitlib.Modify {
			continue
		}

		oldBlob := blobData.BlobCache[change.From.Hash]
		newBlob := blobData.BlobCache[change.To.Hash]

		if oldBlob == nil || newBlob == nil {
			continue
		}

		if oldBlob.IsBinary() || newBlob.IsBinary() {
			continue
		}

		// Check cache for this diff via the Fetcher.
		key := DiffKey{OldHash: change.From.Hash, NewHash: change.To.Hash}

		cached, fetchErr := p.diffFetch.Fetch(ctx, key)
		if fetchErr == nil {
			if cacheHits == nil {
				cacheHits = make(map[string]plumbing.FileDiffData)
			}

			cacheHits[change.To.Name] = cached

			continue
		}

		requests = append(requests, gitlib.DiffRequest{
			OldHash: change.From.Hash,
			NewHash: change.To.Hash,
			OldData: oldBlob.Data,
			NewData: newBlob.Data,
			HasOld:  true,
			HasNew:  true,
		})
		paths = append(paths, change.To.Name)
		changes = append(changes, change)
	}

	req = gitlib.DiffBatchRequest{Requests: requests}

	return req, paths, changes, cacheHits
}

func (p *DiffPipeline) processDiffResponse(
	data CommitData,
	resp gitlib.DiffBatchResponse,
	paths []string,
	changes []*gitlib.Change,
) {
	diffResults := resp.Results

	for i, path := range paths {
		oldBlob := data.BlobCache[changes[i].From.Hash]
		newBlob := data.BlobCache[changes[i].To.Hash]

		// Use Go's counting.
		oldLines, errOld := oldBlob.CountLines()
		newLines, errNew := newBlob.CountLines()

		if errOld != nil || errNew != nil {
			continue
		}

		diffRes := diffResults[i]

		var fileDiff plumbing.FileDiffData

		if diffRes.Error != nil {
			fileDiff = p.fileDiffFromGoDiff(oldBlob, newBlob, oldLines, newLines)
		} else {
			diffs := convertDiffOpsToDMP(diffRes.Ops)
			fileDiff = plumbing.FileDiffData{
				OldLinesOfCode: oldLines,
				NewLinesOfCode: newLines,
				Diffs:          diffs,
			}
		}

		data.FileDiffs[path] = fileDiff

		// Store in cache via the store function.
		key := DiffKey{OldHash: changes[i].From.Hash, NewHash: changes[i].To.Hash}
		p.diffStore(key, fileDiff)
	}
}

func convertDiffOpsToDMP(ops []gitlib.DiffOp) []diffmatchpatch.Diff {
	diffs := make([]diffmatchpatch.Diff, 0, len(ops))

	for _, op := range ops {
		var dmpType diffmatchpatch.Operation

		switch op.Type {
		case gitlib.DiffOpEqual:
			dmpType = diffmatchpatch.DiffEqual
		case gitlib.DiffOpInsert:
			dmpType = diffmatchpatch.DiffInsert
		case gitlib.DiffOpDelete:
			dmpType = diffmatchpatch.DiffDelete
		default:
			continue
		}

		diffs = append(diffs, diffmatchpatch.Diff{
			Type: dmpType,
			Text: strings.Repeat("L", op.LineCount),
		})
	}

	return diffs
}

func (p *DiffPipeline) fileDiffFromGoDiff(oldBlob, newBlob *gitlib.CachedBlob, oldLines, newLines int) plumbing.FileDiffData {
	strFrom, strTo := string(oldBlob.Data), string(newBlob.Data)

	if strFrom == strTo {
		return plumbing.FileDiffData{
			OldLinesOfCode: oldLines,
			NewLinesOfCode: newLines,
			Diffs:          []diffmatchpatch.Diff{{Type: diffmatchpatch.DiffEqual, Text: strings.Repeat("L", oldLines)}},
		}
	}

	dmp := diffmatchpatch.New()
	src, dst, _ := dmp.DiffLinesToRunes(strFrom, strTo)
	diffs := dmp.DiffMainRunes(src, dst, false)
	diffs = dmp.DiffCleanupMerge(dmp.DiffCleanupSemanticLossless(diffs))

	return plumbing.FileDiffData{
		OldLinesOfCode: oldLines,
		NewLinesOfCode: newLines,
		Diffs:          diffs,
	}
}
