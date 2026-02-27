package framework

import (
	"context"
	"os"
	"path/filepath"
	"testing"
	"time"

	git2go "github.com/libgit2/git2go/v34"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/plumbing"
	pkgplumbing "github.com/Sumatoshi-tech/codefang/internal/plumbing"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

// TestRepo is a temporary git repo for framework tests.
type TestRepo struct {
	t    *testing.T
	path string
	repo *git2go.Repository
}

// NewTestRepo creates a temporary git repo for tests.
func NewTestRepo(t *testing.T) *TestRepo {
	t.Helper()
	dir := t.TempDir()

	repo, err := git2go.InitRepository(dir, false)
	if err != nil {
		t.Fatalf("InitRepository: %v", err)
	}

	return &TestRepo{t: t, path: dir, repo: repo}
}

// Close frees the repository.
func (r *TestRepo) Close() {
	if r.repo != nil {
		r.repo.Free()
		r.repo = nil
	}
}

// Path returns the repo directory path.
func (r *TestRepo) Path() string { return r.path }

// CreateFile creates a file with the given content.
func (r *TestRepo) CreateFile(name, content string) {
	r.t.Helper()

	p := filepath.Join(r.path, name)

	err := os.MkdirAll(filepath.Dir(p), 0o750)
	if err != nil {
		r.t.Fatalf("MkdirAll: %v", err)
	}

	err = os.WriteFile(p, []byte(content), 0o600)
	if err != nil {
		r.t.Fatalf("WriteFile: %v", err)
	}
}

// Commit creates a commit with the given message.
func (r *TestRepo) Commit(message string) gitlib.Hash {
	r.t.Helper()

	index, err := r.repo.Index()
	if err != nil {
		r.t.Fatalf("Index: %v", err)
	}
	defer index.Free()

	addErr := index.AddAll([]string{"*"}, git2go.IndexAddDefault, nil)
	if addErr != nil {
		r.t.Fatalf("AddAll: %v", addErr)
	}

	writeErr := index.Write()
	if writeErr != nil {
		r.t.Fatalf("Index.Write: %v", writeErr)
	}

	treeID, writeTreeErr := index.WriteTree()
	if writeTreeErr != nil {
		r.t.Fatalf("WriteTree: %v", writeTreeErr)
	}

	tree, lookupTreeErr := r.repo.LookupTree(treeID)
	if lookupTreeErr != nil {
		r.t.Fatalf("LookupTree: %v", lookupTreeErr)
	}
	defer tree.Free()

	sig := &git2go.Signature{Name: "Test", Email: "test@test.com", When: time.Now()}

	var parents []*git2go.Commit

	head, err := r.repo.Head()
	if err == nil {
		headCommit, lookupErr := r.repo.LookupCommit(head.Target())
		if lookupErr == nil && headCommit != nil {
			parents = append(parents, headCommit)
		}

		head.Free()
	}

	oid, err := r.repo.CreateCommit("HEAD", sig, sig, message, tree, parents...)
	if err != nil {
		r.t.Fatalf("CreateCommit: %v", err)
	}

	for _, p := range parents {
		p.Free()
	}

	return gitlib.HashFromOid(oid)
}

// CommitToRef creates a commit on a branch ref without moving HEAD.
func (r *TestRepo) CommitToRef(refName, message string, parent gitlib.Hash) gitlib.Hash {
	r.t.Helper()

	index, err := r.repo.Index()
	if err != nil {
		r.t.Fatalf("Index: %v", err)
	}
	defer index.Free()

	addErr := index.AddAll([]string{"*"}, git2go.IndexAddDefault, nil)
	if addErr != nil {
		r.t.Fatalf("AddAll: %v", addErr)
	}

	writeErr := index.Write()
	if writeErr != nil {
		r.t.Fatalf("Index.Write: %v", writeErr)
	}

	treeID, writeTreeErr := index.WriteTree()
	if writeTreeErr != nil {
		r.t.Fatalf("WriteTree: %v", writeTreeErr)
	}

	tree, lookupTreeErr := r.repo.LookupTree(treeID)
	if lookupTreeErr != nil {
		r.t.Fatalf("LookupTree: %v", lookupTreeErr)
	}
	defer tree.Free()

	sig := &git2go.Signature{Name: "Test", Email: "test@test.com", When: time.Now()}

	var parents []*git2go.Commit

	if !parent.IsZero() {
		p, lookupErr := r.repo.LookupCommit(parent.ToOid())
		if lookupErr != nil {
			r.t.Fatalf("LookupCommit parent: %v", lookupErr)
		}

		parents = append(parents, p)
	}

	oid, err := r.repo.CreateCommit(refName, sig, sig, message, tree, parents...)
	if err != nil {
		r.t.Fatalf("CreateCommit: %v", err)
	}

	for _, p := range parents {
		p.Free()
	}

	return gitlib.HashFromOid(oid)
}

// CreateMergeCommit creates a merge commit with two parents.
func (r *TestRepo) CreateMergeCommit(message string, firstParent, secondParent gitlib.Hash) gitlib.Hash {
	r.t.Helper()

	parent1, err := r.repo.LookupCommit(firstParent.ToOid())
	if err != nil {
		r.t.Fatalf("LookupCommit first parent: %v", err)
	}
	defer parent1.Free()

	parent2, err := r.repo.LookupCommit(secondParent.ToOid())
	if err != nil {
		r.t.Fatalf("LookupCommit second parent: %v", err)
	}
	defer parent2.Free()

	tree, err := parent1.Tree()
	if err != nil {
		r.t.Fatalf("Tree: %v", err)
	}
	defer tree.Free()

	sig := &git2go.Signature{Name: "Test", Email: "test@test.com", When: time.Now()}

	oid, err := r.repo.CreateCommit("HEAD", sig, sig, message, tree, parent1, parent2)
	if err != nil {
		r.t.Fatalf("CreateCommit: %v", err)
	}

	return gitlib.HashFromOid(oid)
}

// CollectCommits returns up to limit commits (newest first) from the repo.
func CollectCommits(t *testing.T, repo *gitlib.Repository, limit int) []*gitlib.Commit {
	t.Helper()

	return collectCommitsWithOpts(t, repo, limit, &gitlib.LogOptions{})
}

// CollectCommitsFirstParent returns up to limit commits using first-parent walk.
func CollectCommitsFirstParent(t *testing.T, repo *gitlib.Repository, limit int) []*gitlib.Commit {
	t.Helper()

	return collectCommitsWithOpts(t, repo, limit, &gitlib.LogOptions{FirstParent: true})
}

func collectCommitsWithOpts(t *testing.T, repo *gitlib.Repository, limit int, opts *gitlib.LogOptions) []*gitlib.Commit {
	t.Helper()

	iter, err := repo.Log(opts)
	if err != nil {
		t.Fatalf("Log: %v", err)
	}

	out := make([]*gitlib.Commit, 0, max(limit, 1))
	for limit <= 0 || len(out) < limit {
		commit, nextErr := iter.Next()
		if nextErr != nil {
			break
		}

		out = append(out, commit)
	}

	iter.Close()

	return out
}

// FileDiffFromGoDiffForTest exposes fileDiffFromGoDiff for tests.
func FileDiffFromGoDiffForTest(
	p *DiffPipeline,
	oldBlob, newBlob *gitlib.CachedBlob,
	oldLines, newLines int,
) pkgplumbing.FileDiffData {
	return p.fileDiffFromGoDiff(oldBlob, newBlob, oldLines, newLines)
}

// RunnerBallastSizeForTest exposes runtime ballast size retained by a runner.
func RunnerBallastSizeForTest(runner *Runner) int {
	return len(runner.runtimeBallast)
}

// ResolveMemoryLimitForTest exposes memory limit resolution logic.
func ResolveMemoryLimitForTest(totalMemoryBytes uint64) uint64 {
	return resolveMemoryLimit(totalMemoryBytes)
}

// ResolveMemoryLimitFromBudgetForTest exposes budget-aligned memory limit logic.
func ResolveMemoryLimitFromBudgetForTest(budget int64, totalMemoryBytes uint64) uint64 {
	return resolveMemoryLimitFromBudget(budget, totalMemoryBytes)
}

// SplitLeavesForTest exposes the three-group leaf split for testing.
func SplitLeavesForTest(runner *Runner) (cpuHeavy, lightweight, serial []analyze.HistoryAnalyzer) {
	return runner.splitLeaves()
}

// RecordCommitMetaForTest exposes recordCommitMeta for unit testing.
func RecordCommitMetaForTest(runner *Runner, tc analyze.TC) {
	runner.recordCommitMeta(tc)
}

// AuthorNameForTest exposes authorName for unit testing.
func AuthorNameForTest(runner *Runner, authorID int) string {
	return runner.authorName(authorID)
}

// CommitMetaForTest returns the accumulated commit metadata map.
func CommitMetaForTest(runner *Runner) map[string]analyze.CommitMeta {
	return runner.commitMeta
}

// InjectCommitMetaForTest exposes injectCommitMeta for unit testing.
func InjectCommitMetaForTest(runner *Runner, reports map[analyze.HistoryAnalyzer]analyze.Report) {
	runner.injectCommitMeta(reports)
}

// InitAggregatorsForTest exposes initAggregators for unit testing.
func InitAggregatorsForTest(runner *Runner) {
	runner.initAggregators()
}

// SetIDProviderForTest sets the identity provider on a Runner for testing.
func SetIDProviderForTest(runner *Runner, id *plumbing.IdentityDetector) {
	runner.idProvider = id
}

// AggregatorsForTest returns the aggregator slice from the runner.
func AggregatorsForTest(runner *Runner) []analyze.Aggregator {
	return runner.aggregators
}

// AddTCForTest exposes addTC for unit testing.
func AddTCForTest(runner *Runner, tc analyze.TC, idx int, ac *analyze.Context) {
	runner.addTC(tc, idx, ac)
}

// AggSpillBudgetForTest returns the runner's aggregator spill budget.
func AggSpillBudgetForTest(runner *Runner) int64 {
	return runner.AggSpillBudget
}

// AggregatorStateSizeForTest exposes AggregatorStateSize for unit testing.
func AggregatorStateSizeForTest(runner *Runner) int64 {
	return runner.AggregatorStateSize()
}

// TCCountAccumulatedForTest exposes TCCountAccumulated for unit testing.
func TCCountAccumulatedForTest(runner *Runner) int64 {
	return runner.TCCountAccumulated()
}

// ResetTCCountForTest exposes ResetTCCount for unit testing.
func ResetTCCountForTest(runner *Runner) {
	runner.ResetTCCount()
}

// NewRunner creates a new Runner for the given repository and analyzers.
// Uses DefaultCoordinatorConfig(). Use NewRunnerWithConfig for custom configuration.
func NewRunner(repo *gitlib.Repository, repoPath string, analyzers ...analyze.HistoryAnalyzer) *Runner {
	return NewRunnerWithConfig(repo, repoPath, DefaultCoordinatorConfig(), analyzers...)
}

// NewBlobPipeline creates a new blob pipeline without cache (test convenience).
func NewBlobPipeline(
	seqChan chan<- gitlib.WorkerRequest,
	poolChan chan<- gitlib.WorkerRequest,
	bufferSize int,
	workerCount int,
) *BlobPipeline {
	return NewBlobPipelineWithCache(seqChan, poolChan, bufferSize, workerCount, nil)
}

// NewDiffPipeline creates a new diff pipeline without cache (test convenience).
func NewDiffPipeline(workerChan chan<- gitlib.WorkerRequest, bufferSize int) *DiffPipeline {
	return NewDiffPipelineWithCache(workerChan, bufferSize, nil)
}

// NewCommitStreamer creates a new commit streamer with default settings.
func NewCommitStreamer() *CommitStreamer {
	return &CommitStreamer{
		BatchSize: defaultBatchSize,
		Lookahead: defaultLookahead,
	}
}

// iteratorStreamState holds state for streaming from an iterator.
type iteratorStreamState struct {
	streamer   *CommitStreamer
	iter       *gitlib.CommitIter
	out        chan<- CommitBatch
	limit      int
	batchID    int
	startIndex int
	count      int
}

// collectBatch collects up to BatchSize commits from the iterator.
func (st *iteratorStreamState) collectBatch() []*gitlib.Commit {
	batch := make([]*gitlib.Commit, 0, st.streamer.BatchSize)

	for len(batch) < st.streamer.BatchSize {
		if st.limit > 0 && st.count >= st.limit {
			break
		}

		commit, err := st.iter.Next()
		if err != nil {
			break
		}

		batch = append(batch, commit)
		st.count++
	}

	return batch
}

// sendBatch sends a batch to the output channel.
func (st *iteratorStreamState) sendBatch(ctx context.Context, batch []*gitlib.Commit) bool {
	commitBatch := CommitBatch{
		Commits:    batch,
		StartIndex: st.startIndex,
		BatchID:    st.batchID,
	}

	select {
	case st.out <- commitBatch:
		st.batchID++
		st.startIndex += len(batch)

		return true
	case <-ctx.Done():
		return false
	}
}

// StreamFromIterator streams commits from a commit iterator.
func (s *CommitStreamer) StreamFromIterator(ctx context.Context, iter *gitlib.CommitIter, limit int) <-chan CommitBatch {
	out := make(chan CommitBatch, s.Lookahead)

	go func() {
		defer close(out)
		defer iter.Close()

		st := &iteratorStreamState{streamer: s, iter: iter, out: out, limit: limit}

		for {
			batch := st.collectBatch()
			if len(batch) == 0 {
				return
			}

			if !st.sendBatch(ctx, batch) {
				return
			}

			if limit > 0 && st.count >= limit {
				return
			}
		}
	}()

	return out
}

// StreamSingle streams commits one at a time (batch size = 1).
func (s *CommitStreamer) StreamSingle(ctx context.Context, commits []*gitlib.Commit) <-chan CommitBatch {
	out := make(chan CommitBatch, s.Lookahead)

	go func() {
		defer close(out)

		for i, commit := range commits {
			batch := CommitBatch{
				Commits:    []*gitlib.Commit{commit},
				StartIndex: i,
				BatchID:    i,
			}

			select {
			case out <- batch:
			case <-ctx.Done():
				return
			}
		}
	}()

	return out
}

// ProcessSingleForTest processes a single commit via the coordinator.
func (c *Coordinator) ProcessSingle(ctx context.Context, commit *gitlib.Commit, _ int) CommitData {
	commits := []*gitlib.Commit{commit}
	ch := c.Process(ctx, commits)

	return <-ch
}

// ConfigForTest returns the coordinator configuration.
func (c *Coordinator) Config() CoordinatorConfig {
	return c.config
}
