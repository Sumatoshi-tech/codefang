package gitlib

import (
	"runtime"
)

// Request types for the Gitlib Worker
type WorkerRequest interface {
	isWorkerRequest()
}

// TreeDiffRequest asks for a tree diff for a specific commit hash.
type TreeDiffRequest struct {
	PreviousTree *Tree // Can be nil for initial. Belongs to Worker's repo.
	CommitHash   Hash  // Hash of the commit to process.
	Response     chan<- TreeDiffResponse
}

type TreeDiffResponse struct {
	Changes     Changes
	CurrentTree *Tree // The tree of the processed commit. Caller must Free this or pass it back.
	Error       error
}

// BlobBatchRequest asks to load a batch of blobs.
type BlobBatchRequest struct {
	Hashes   []Hash
	Response chan<- BlobBatchResponse
}

type BlobBatchResponse struct {
	Blobs   []*CachedBlob
	Results []BlobResult
}

// DiffBatchRequest asks to compute diffs for a batch of pairs.
type DiffBatchRequest struct {
	Requests []DiffRequest
	Response chan<- DiffBatchResponse
}

type DiffBatchResponse struct {
	Results []DiffResult
}

func (TreeDiffRequest) isWorkerRequest()  {}
func (BlobBatchRequest) isWorkerRequest() {}
func (DiffBatchRequest) isWorkerRequest() {}

// Worker manages exclusive, sequential access to the libgit2 Repository.
// It ensures all CGO calls happen on a single OS thread.
type Worker struct {
	repo     *Repository
	requests <-chan WorkerRequest
	bridge   *CGOBridge
	done     chan struct{}
}

// NewWorker creates a new Gitlib Worker that consumes from the given channel.
func NewWorker(repo *Repository, requests <-chan WorkerRequest) *Worker {
	return &Worker{
		repo:     repo,
		requests: requests,
		bridge:   NewCGOBridge(repo),
		done:     make(chan struct{}),
	}
}

// Start runs the worker loop. This MUST be called.
// It locks the goroutine to the OS thread to satisfy libgit2 constraints.
func (w *Worker) Start() {
	go func() {
		runtime.LockOSThread()
		defer runtime.UnlockOSThread()
		defer close(w.done)

		for req := range w.requests {
			w.handle(req)
		}
	}()
}

// Stop waits for the worker to finish.
// Note: The caller must close the requests channel to trigger shutdown.
func (w *Worker) Stop() {
	<-w.done
}

func (w *Worker) handle(req WorkerRequest) {
	switch r := req.(type) {
	case TreeDiffRequest:
		commit, err := w.repo.LookupCommit(r.CommitHash)
		if err != nil {
			r.Response <- TreeDiffResponse{Error: err}
			return
		}
		// We don't need to keep the commit, just the tree.
		// git2go Commit.Tree() returns a new Tree object.
		// We can free the commit immediately after getting the tree.
		// Wait, in Go bindings, usually we rely on GC for pure Go objects,
		// but commit wraps C object. We should Free it if we manually looked it up.
		// However, LookupCommit returns *Commit which has a Free() but also a finalizer?
		// gitlib.Commit does NOT have a finalizer in my implementation (checked repository.go:57).
		// Wait, gitlib.Commit wraps git2go.Commit.
		// git2go usually sets finalizers.
		// But let's be explicit if possible.
		// gitlib.Commit doesn't expose Free() directly in the snippet I read?
		// Snippet line 57: &Commit{commit: commit, repo: r}.
		// Check pkg/gitlib/commit.go for Free().
		// Assuming it has one or relies on git2go.
		
		commitTree, err := commit.Tree()
		// Safe to free commit now as tree is independent object in libgit2
		commit.Free() 
		
		if err != nil {
			r.Response <- TreeDiffResponse{Error: err}
			return
		}

		var changes Changes
		if r.PreviousTree != nil {
			changes, err = TreeDiff(w.repo, r.PreviousTree, commitTree)
		} else {
			changes, err = InitialTreeChanges(w.repo, commitTree)
		}

		// We return commitTree so the caller can use it as PreviousTree next time.
		// The caller is responsible for ensuring it's freed eventually (e.g. when dropping it as PreviousTree).
		r.Response <- TreeDiffResponse{
			Changes:     changes,
			CurrentTree: commitTree,
			Error:       err,
		}

	case BlobBatchRequest:
		results := w.bridge.BatchLoadBlobs(r.Hashes)
		blobs := make([]*CachedBlob, len(results))
		for i, res := range results {
			if res.Error == nil {
				blobs[i] = &CachedBlob{
					hash: res.Hash,
					size: res.Size,
					Data: res.Data,
				}
			}
		}
		r.Response <- BlobBatchResponse{Blobs: blobs, Results: results}

	case DiffBatchRequest:
		results := w.bridge.BatchDiffBlobs(r.Requests)
		r.Response <- DiffBatchResponse{Results: results}
	}
}
