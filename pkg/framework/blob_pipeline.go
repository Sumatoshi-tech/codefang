package framework

import (
	"context"

	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

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
}

// NewBlobPipeline creates a new blob pipeline.
func NewBlobPipeline(
	seqChan chan<- gitlib.WorkerRequest,
	poolChan chan<- gitlib.WorkerRequest,
	bufferSize int,
) *BlobPipeline {
	if bufferSize <= 0 {
		bufferSize = 1
	}

	return &BlobPipeline{
		SeqWorkerChan:  seqChan,
		PoolWorkerChan: poolChan,
		BufferSize:     bufferSize,
	}
}

type blobJob struct {
	data     BlobData
	respChan chan gitlib.BlobBatchResponse
	hashes   []gitlib.Hash
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

	var previousTree *gitlib.Tree

	defer func() {
		if previousTree != nil {
			previousTree.Free()
		}
	}()

	for batch := range commits {
		select {
		case <-ctx.Done():
			return
		default:
		}

		previousTree = p.processBatch(ctx, batch, previousTree, jobs)
		if previousTree == nil && ctx.Err() != nil {
			return
		}
	}
}

// processBatch processes a single commit batch and returns the updated previous tree.
func (p *BlobPipeline) processBatch(
	ctx context.Context, batch CommitBatch, previousTree *gitlib.Tree, jobs chan<- blobJob,
) *gitlib.Tree {
	for i, commit := range batch.Commits {
		changes, currentTree, err := p.doTreeDiff(commit, previousTree)

		if previousTree != nil {
			previousTree.Free()
		}

		previousTree = currentTree

		job := blobJob{
			data: BlobData{
				Commit:  commit,
				Index:   batch.StartIndex + i,
				Changes: changes,
				Error:   err,
			},
		}

		if err == nil {
			if !p.fireBlobRequest(ctx, &job, changes) {
				return previousTree
			}
		}

		select {
		case jobs <- job:
		case <-ctx.Done():
			return previousTree
		}
	}

	return previousTree
}

// fireBlobRequest initiates a parallel blob load request.
func (p *BlobPipeline) fireBlobRequest(ctx context.Context, job *blobJob, changes gitlib.Changes) bool {
	req, hashes := p.prepareBlobRequest(changes)
	job.hashes = hashes

	if len(hashes) == 0 {
		return true
	}

	job.respChan = make(chan gitlib.BlobBatchResponse, 1)
	req.Response = job.respChan

	select {
	case p.PoolWorkerChan <- req:
		return true
	case <-ctx.Done():
		return false
	}
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
	if job.respChan == nil {
		job.data.BlobCache = make(map[gitlib.Hash]*gitlib.CachedBlob)

		return true
	}

	select {
	case resp := <-job.respChan:
		cache := make(map[gitlib.Hash]*gitlib.CachedBlob, len(resp.Blobs))

		for i, blob := range resp.Blobs {
			if blob != nil {
				cache[job.hashes[i]] = blob
			}
		}

		job.data.BlobCache = cache

		return true
	case <-ctx.Done():
		return false
	}
}

func (p *BlobPipeline) doTreeDiff(commit *gitlib.Commit, previousTree *gitlib.Tree) (gitlib.Changes, *gitlib.Tree, error) {
	respChan := make(chan gitlib.TreeDiffResponse, 1)
	p.SeqWorkerChan <- gitlib.TreeDiffRequest{
		PreviousTree: previousTree,
		CommitHash:   commit.Hash(),
		Response:     respChan,
	}

	resp := <-respChan

	return resp.Changes, resp.CurrentTree, resp.Error
}

func (p *BlobPipeline) prepareBlobRequest(changes gitlib.Changes) (gitlib.BlobBatchRequest, []gitlib.Hash) {
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

	return gitlib.BlobBatchRequest{Hashes: hashes}, hashes
}
