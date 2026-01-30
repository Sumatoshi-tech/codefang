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

	// Job channel for ordered results
	jobs := make(chan blobJob, p.BufferSize)

	// Producer: Sequential TreeDiff -> Parallel Blob Request
	go func() {
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

			for i, commit := range batch.Commits {
				// 1. Sequential Tree Diff
				// Must block here to maintain order of trees
				changes, currentTree, err := p.doTreeDiff(commit, previousTree)
				
				// Update tree state for next iteration
				if previousTree != nil {
					previousTree.Free()
				}
				previousTree = currentTree // Transferred ownership

				job := blobJob{
					data: BlobData{
						Commit:  commit,
						Index:   batch.StartIndex + i,
						Changes: changes,
						Error:   err,
					},
				}

				if err == nil {
					// 2. Fire Parallel Blob Load
					req, hashes := p.prepareBlobRequest(changes)
					job.hashes = hashes
					
					if len(hashes) > 0 {
						job.respChan = make(chan gitlib.BlobBatchResponse, 1)
						req.Response = job.respChan
						
						select {
						case p.PoolWorkerChan <- req:
						case <-ctx.Done():
							return
						}
					}
				}

				// Push to job queue (blocks if buffer full, throttling producer)
				select {
				case jobs <- job:
				case <-ctx.Done():
					return
				}
			}
		}
	}()

	// Consumer: Wait for Blobs -> Output
	go func() {
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

			if job.respChan != nil {
				// Wait for pool worker result
				select {
				case resp := <-job.respChan:
					// Build cache
					cache := make(map[gitlib.Hash]*gitlib.CachedBlob, len(resp.Blobs))
					for i, blob := range resp.Blobs {
						if blob != nil {
							cache[job.hashes[i]] = blob
						}
					}
					job.data.BlobCache = cache
				case <-ctx.Done():
					return
				}
			} else {
				job.data.BlobCache = make(map[gitlib.Hash]*gitlib.CachedBlob)
			}

			select {
			case out <- job.data:
			case <-ctx.Done():
				return
			}
		}
	}()

	return out
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
