package framework

import (
	"context"
	"maps"
	"strings"
	"sync"

	"github.com/sergi/go-diff/diffmatchpatch"

	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
	"github.com/Sumatoshi-tech/codefang/pkg/plumbing"
	"github.com/Sumatoshi-tech/codefang/pkg/uast"
)

// CommitData holds all processed data for a commit.
type CommitData struct {
	Commit      *gitlib.Commit
	Index       int
	Changes     gitlib.Changes
	BlobCache   map[gitlib.Hash]*gitlib.CachedBlob
	FileDiffs   map[string]plumbing.FileDiffData
	UASTChanges []uast.Change // Pre-computed UAST changes (nil if not computed).
	Error       error
}

// DiffPipeline processes blob data to compute file diffs.
type DiffPipeline struct {
	PoolWorkerChan chan<- gitlib.WorkerRequest
	BufferSize     int
	DiffCache      *DiffCache
}

// NewDiffPipeline creates a new diff pipeline.
func NewDiffPipeline(workerChan chan<- gitlib.WorkerRequest, bufferSize int) *DiffPipeline {
	return NewDiffPipelineWithCache(workerChan, bufferSize, nil)
}

// NewDiffPipelineWithCache creates a new diff pipeline with an optional diff cache.
func NewDiffPipelineWithCache(workerChan chan<- gitlib.WorkerRequest, bufferSize int, cache *DiffCache) *DiffPipeline {
	if bufferSize <= 0 {
		bufferSize = 1
	}

	return &DiffPipeline{
		PoolWorkerChan: workerChan,
		BufferSize:     bufferSize,
		DiffCache:      cache,
	}
}

type diffJob struct {
	data      CommitData
	paths     []string                         // paths for diffs requested from C.
	changes   []*gitlib.Change                 // changes for diffs requested from C.
	cacheHits map[string]plumbing.FileDiffData // path -> cached diff.

	// Batching fields for cross-commit batching.
	pendingRequests []gitlib.DiffRequest
	batchResp       *sharedDiffResponse
	batchOffset     int
	batchLen        int
}

// Process receives blob data and outputs commit data with computed diffs.
func (p *DiffPipeline) Process(ctx context.Context, blobs <-chan BlobData) <-chan CommitData {
	out := make(chan CommitData)
	// diffJobBufferMultiplier scales the job buffer relative to pipeline buffer size.
	// A larger buffer allows accumulating more diff jobs for cross-commit batching.
	const diffJobBufferMultiplier = 10

	// Larger buffer for jobs to accumulate batch.
	jobs := make(chan diffJob, p.BufferSize*diffJobBufferMultiplier)

	go p.runDiffProducer(ctx, blobs, jobs)
	go p.runDiffConsumer(ctx, jobs, out)

	return out
}

// runDiffProducer processes blob data and creates diff jobs.
func (p *DiffPipeline) runDiffProducer(ctx context.Context, blobs <-chan BlobData, jobs chan<- diffJob) {
	defer close(jobs)

	// We accumulate diff requests until we have a decent batch size (e.g. 200 diffs)
	// or until input channel is dry.
	// Since BlobPipeline emits BlobData which already contains multiple diffs per commit,
	// we are effectively re-batching across commits.

	const maxBatchSize = 1000

	var (
		currentBatchReqs []gitlib.DiffRequest
		currentBatchJobs []*diffJob
	)

	flushBatch := func() {
		if len(currentBatchJobs) == 0 {
			return
		}

		var sharedResp *sharedDiffResponse

		// Only fire CGO request if there are actual diff requests.
		if len(currentBatchReqs) > 0 {
			req := gitlib.DiffBatchRequest{Requests: currentBatchReqs}
			respChan := make(chan gitlib.DiffBatchResponse, 1)
			req.Response = respChan

			// Send request.
			select {
			case p.PoolWorkerChan <- req:
			case <-ctx.Done():
				return
			}

			// Create a shared state for this batch.
			sharedResp = &sharedDiffResponse{
				respChan: respChan,
			}
		}

		// Assign shared response to all jobs and dispatch.
		startIdx := 0

		for _, job := range currentBatchJobs {
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

		// Reset batch.
		currentBatchReqs = nil
		currentBatchJobs = nil
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

		if len(reqs) > 0 {
			currentBatchReqs = append(currentBatchReqs, reqs...)
			job.pendingRequests = reqs // Keep track for offset calculation.
		}

		currentBatchJobs = append(currentBatchJobs, job)

		if len(currentBatchReqs) >= maxBatchSize {
			flushBatch()
		}
	}

	// Flush remaining.
	flushBatch()
}

type sharedDiffResponse struct {
	respChan chan gitlib.DiffBatchResponse
	results  []gitlib.DiffResult
	err      error
	once     sync.Once
}

func (s *sharedDiffResponse) wait(ctx context.Context) {
	s.once.Do(func() {
		select {
		case resp := <-s.respChan:
			s.results = resp.Results
		case <-ctx.Done():
			s.err = ctx.Err()
		}
	})
}

// createDiffJobInternal prepares the job but doesn't fire requests.
func (p *DiffPipeline) createDiffJobInternal(_ context.Context, blobData BlobData) (*diffJob, []gitlib.DiffRequest) {
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

	req, paths, changes, cacheHits := p.prepareDiffRequest(blobData)
	job.paths = paths
	job.changes = changes
	job.cacheHits = cacheHits

	return job, req.Requests
}

// runDiffConsumer waits for diff responses and outputs commit data.
func (p *DiffPipeline) runDiffConsumer(ctx context.Context, jobs <-chan diffJob, out chan<- CommitData) {
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

		job.data.FileDiffs = make(map[string]plumbing.FileDiffData)

		// Add cache hits first.
		maps.Copy(job.data.FileDiffs, job.cacheHits)

		// Process batched diff response.
		if job.batchResp != nil && job.batchLen > 0 {
			job.batchResp.wait(ctx)

			if job.batchResp.err != nil {
				job.data.Error = job.batchResp.err
			} else {
				// Extract this job's portion of results.
				batchResults := job.batchResp.results
				if job.batchOffset+job.batchLen <= len(batchResults) {
					jobResults := batchResults[job.batchOffset : job.batchOffset+job.batchLen]
					resp := gitlib.DiffBatchResponse{Results: jobResults}
					p.processDiffResponse(job.data, resp, job.paths, job.changes)
				}
			}
		}

		select {
		case out <- job.data:
		case <-ctx.Done():
			return
		}
	}
}

func (p *DiffPipeline) prepareDiffRequest(blobData BlobData) (
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

		// Check cache for this diff.
		if p.DiffCache != nil {
			key := DiffKey{OldHash: change.From.Hash, NewHash: change.To.Hash}
			if cached, found := p.DiffCache.Get(key); found {
				if cacheHits == nil {
					cacheHits = make(map[string]plumbing.FileDiffData)
				}

				cacheHits[change.To.Name] = cached

				continue
			}
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

		// Store in cache.
		if p.DiffCache != nil {
			key := DiffKey{OldHash: changes[i].From.Hash, NewHash: changes[i].To.Hash}
			p.DiffCache.Put(key, fileDiff)
		}
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
