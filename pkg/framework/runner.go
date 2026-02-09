// Package framework provides orchestration for running analysis pipelines.
package framework

import (
	"context"
	"errors"
	"fmt"
	"sync"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

// ErrNotParallelizable is returned when a leaf analyzer does not implement [analyze.Parallelizable].
var ErrNotParallelizable = errors.New("leaf does not implement Parallelizable")

// Runner orchestrates multiple HistoryAnalyzers over a commit sequence.
// It always uses the Coordinator pipeline (batch blob load + batch diff in C).
type Runner struct {
	Repo      *gitlib.Repository
	RepoPath  string
	Analyzers []analyze.HistoryAnalyzer
	Config    CoordinatorConfig

	// CoreCount is the number of leading analyzers in the Analyzers slice that are
	// core (plumbing) analyzers. These run sequentially. Analyzers after CoreCount
	// are leaf analyzers that can be parallelized via Fork/Merge.
	// Set to 0 to disable parallel leaf consumption.
	CoreCount int

	runtimeTuningOnce sync.Once
	runtimeBallast    []byte
}

// NewRunner creates a new Runner for the given repository and analyzers.
// Uses DefaultCoordinatorConfig(). Use NewRunnerWithConfig for custom configuration.
func NewRunner(repo *gitlib.Repository, repoPath string, analyzers ...analyze.HistoryAnalyzer) *Runner {
	return NewRunnerWithConfig(repo, repoPath, DefaultCoordinatorConfig(), analyzers...)
}

// NewRunnerWithConfig creates a new Runner with custom coordinator configuration.
func NewRunnerWithConfig(
	repo *gitlib.Repository,
	repoPath string,
	config CoordinatorConfig,
	analyzers ...analyze.HistoryAnalyzer,
) *Runner {
	return &Runner{
		Repo:      repo,
		RepoPath:  repoPath,
		Analyzers: analyzers,
		Config:    config,
	}
}

// Run executes all analyzers over the given commits: initialize, consume each commit via pipeline, then finalize.
func (runner *Runner) Run(commits []*gitlib.Commit) (map[analyze.HistoryAnalyzer]analyze.Report, error) {
	for _, a := range runner.Analyzers {
		err := a.Initialize(runner.Repo)
		if err != nil {
			return nil, err
		}
	}

	if len(commits) == 0 {
		reports := make(map[analyze.HistoryAnalyzer]analyze.Report)

		for _, a := range runner.Analyzers {
			rep, err := a.Finalize()
			if err != nil {
				return nil, err
			}

			reports[a] = rep
		}

		return reports, nil
	}

	return runner.runInternal(commits)
}

// runInternal uses the Coordinator to process commits (batch blob + batch diff in C), then feeds analyzers.
func (runner *Runner) runInternal(commits []*gitlib.Commit) (map[analyze.HistoryAnalyzer]analyze.Report, error) {
	processErr := runner.processCommits(commits, 0)
	if processErr != nil {
		return nil, processErr
	}

	reports := make(map[analyze.HistoryAnalyzer]analyze.Report)

	for _, a := range runner.Analyzers {
		rep, finalizeErr := a.Finalize()
		if finalizeErr != nil {
			return nil, finalizeErr
		}

		reports[a] = rep
	}

	return reports, nil
}

// Initialize initializes all analyzers. Call once before processing chunks.
func (runner *Runner) Initialize() error {
	for _, a := range runner.Analyzers {
		err := a.Initialize(runner.Repo)
		if err != nil {
			return err
		}
	}

	return nil
}

// ProcessChunk processes a chunk of commits without Initialize/Finalize.
// Use this for streaming mode where Initialize is called once at start
// and Finalize once at end.
// The indexOffset is added to the commit index to maintain correct ordering across chunks.
func (runner *Runner) ProcessChunk(commits []*gitlib.Commit, indexOffset int) error {
	return runner.processCommits(commits, indexOffset)
}

// Finalize finalizes all analyzers and returns their reports.
func (runner *Runner) Finalize() (map[analyze.HistoryAnalyzer]analyze.Report, error) {
	reports := make(map[analyze.HistoryAnalyzer]analyze.Report)

	for _, a := range runner.Analyzers {
		rep, err := a.Finalize()
		if err != nil {
			return nil, err
		}

		reports[a] = rep
	}

	return reports, nil
}

// processCommits processes commits through the pipeline without Initialize/Finalize.
func (runner *Runner) processCommits(commits []*gitlib.Commit, indexOffset int) error {
	runner.runtimeTuningOnce.Do(func() {
		runner.runtimeBallast = applyRuntimeTuning(runner.Config)
	})

	w := runner.Config.LeafWorkers
	if w > 0 && runner.CoreCount > 0 && runner.CoreCount < len(runner.Analyzers) && !runner.hasSequentialLeaf() {
		return runner.processCommitsParallel(commits, indexOffset)
	}

	return runner.processCommitsSerial(commits, indexOffset)
}

// hasSequentialLeaf returns true if any leaf analyzer requires sequential commit processing
// and cannot be parallelized via round-robin Fork/Merge.
func (runner *Runner) hasSequentialLeaf() bool {
	for _, a := range runner.Analyzers[runner.CoreCount:] {
		if p, ok := a.(analyze.Parallelizable); ok && p.SequentialOnly() {
			return true
		}
	}

	return false
}

// processCommitsSerial is the original serial consumption path.
func (runner *Runner) processCommitsSerial(commits []*gitlib.Commit, indexOffset int) error {
	ctx := context.Background()
	coordinator := NewCoordinator(runner.Repo, runner.Config)
	dataChan := coordinator.Process(ctx, commits)

	for data := range dataChan {
		if data.Error != nil {
			return data.Error
		}

		commit := data.Commit
		analyzeCtx := &analyze.Context{
			Commit:      commit,
			Index:       data.Index + indexOffset,
			Time:        commit.Committer().When,
			IsMerge:     commit.NumParents() > 1,
			Changes:     data.Changes,
			BlobCache:   data.BlobCache,
			FileDiffs:   data.FileDiffs,
			UASTChanges: data.UASTChanges,
		}

		for _, a := range runner.Analyzers {
			err := a.Consume(analyzeCtx)
			if err != nil {
				return err
			}
		}
	}

	return nil
}

// leafWork holds an opaque plumbing snapshot and context for one commit.
type leafWork struct {
	ctx      *analyze.Context
	snapshot analyze.PlumbingSnapshot
}

// leafWorker holds forked leaf analyzers for one worker goroutine.
type leafWorker struct {
	leaves   []analyze.HistoryAnalyzer
	workChan chan leafWork
}

// processWork applies the plumbing snapshot, runs leaf Consume(), then releases snapshot resources.
func (w *leafWorker) processWork(work leafWork) error {
	for _, leaf := range w.leaves {
		p, ok := leaf.(analyze.Parallelizable)
		if !ok {
			return fmt.Errorf("%w: %s", ErrNotParallelizable, leaf.Name())
		}

		p.ApplySnapshot(work.snapshot)

		consumeErr := leaf.Consume(work.ctx)
		if consumeErr != nil {
			return consumeErr
		}
	}

	// Release snapshot resources (e.g. UAST trees). Any leaf can do it.
	p, ok := w.leaves[0].(analyze.Parallelizable)
	if !ok {
		return fmt.Errorf("%w: %s", ErrNotParallelizable, w.leaves[0].Name())
	}

	p.ReleaseSnapshot(work.snapshot)

	return nil
}

// newLeafWorkers creates W leaf workers with forked leaf analyzers.
// Each forked leaf owns independent plumbing struct copies (created by Fork()).
func newLeafWorkers(leaves []analyze.HistoryAnalyzer, w int) []*leafWorker {
	workers := make([]*leafWorker, w)

	for i := range w {
		worker := &leafWorker{
			workChan: make(chan leafWork, 2),
		}

		worker.leaves = make([]analyze.HistoryAnalyzer, len(leaves))

		for j, leaf := range leaves {
			forked := leaf.Fork(1)
			worker.leaves[j] = forked[0]
		}

		workers[i] = worker
	}

	return workers
}

// startLeafWorkers launches goroutines that drain work channels and returns
// a WaitGroup and an error slice (one per worker).
func startLeafWorkers(workers []*leafWorker) (*sync.WaitGroup, []error) {
	numWorkers := len(workers)

	var wg sync.WaitGroup

	workerErrors := make([]error, numWorkers)

	wg.Add(numWorkers)

	for idx, wk := range workers {
		go func(workerIdx int, worker *leafWorker) {
			defer wg.Done()

			for work := range worker.workChan {
				processErr := worker.processWork(work)
				if processErr != nil {
					workerErrors[workerIdx] = processErr

					// Drain remaining work to prevent deadlock.
					for range worker.workChan {
					}

					return
				}
			}
		}(idx, wk)
	}

	return &wg, workerErrors
}

// mergeLeafResults merges forked leaf results back into the original leaf analyzers.
func mergeLeafResults(leaves []analyze.HistoryAnalyzer, workers []*leafWorker) {
	numWorkers := len(workers)

	for leafIdx, leaf := range leaves {
		forks := make([]analyze.HistoryAnalyzer, numWorkers)
		for workerIdx, worker := range workers {
			forks[workerIdx] = worker.leaves[leafIdx]
		}

		leaf.Merge(forks)
	}
}

// processCommitsParallel processes commits with parallel leaf consumption.
// Core (plumbing) analyzers run sequentially; leaf analyzers run on W parallel workers.
func (runner *Runner) processCommitsParallel(commits []*gitlib.Commit, indexOffset int) error {
	ctx := context.Background()
	coordinator := NewCoordinator(runner.Repo, runner.Config)
	dataChan := coordinator.Process(ctx, commits)

	core := runner.Analyzers[:runner.CoreCount]
	leaves := runner.Analyzers[runner.CoreCount:]

	numWorkers := runner.Config.LeafWorkers
	workers := newLeafWorkers(leaves, numWorkers)
	wg, workerErrors := startLeafWorkers(workers)

	// Serial plumbing loop with round-robin dispatch to leaf workers.
	loopErr := runner.consumeAndDispatch(dataChan, core, leaves, workers, numWorkers, indexOffset)

	// Close all work channels to signal workers to finish.
	for _, worker := range workers {
		close(worker.workChan)
	}

	wg.Wait()

	if loopErr != nil {
		return loopErr
	}

	for _, workerErr := range workerErrors {
		if workerErr != nil {
			return workerErr
		}
	}

	mergeLeafResults(leaves, workers)

	return nil
}

// consumeAndDispatch runs core analyzers sequentially and dispatches leaf work round-robin.
func (runner *Runner) consumeAndDispatch(
	dataChan <-chan CommitData,
	core []analyze.HistoryAnalyzer,
	leaves []analyze.HistoryAnalyzer,
	workers []*leafWorker,
	numWorkers int,
	indexOffset int,
) error {
	snapshotter, ok := leaves[0].(analyze.Parallelizable)
	if !ok {
		return fmt.Errorf("%w: %s", ErrNotParallelizable, leaves[0].Name())
	}

	var commitIdx int

	for data := range dataChan {
		if data.Error != nil {
			return data.Error
		}

		commit := data.Commit
		analyzeCtx := &analyze.Context{
			Commit:      commit,
			Index:       data.Index + indexOffset,
			Time:        commit.Committer().When,
			IsMerge:     commit.NumParents() > 1,
			Changes:     data.Changes,
			BlobCache:   data.BlobCache,
			FileDiffs:   data.FileDiffs,
			UASTChanges: data.UASTChanges,
		}

		for _, analyzer := range core {
			consumeErr := analyzer.Consume(analyzeCtx)
			if consumeErr != nil {
				return consumeErr
			}
		}

		work := leafWork{
			ctx:      analyzeCtx,
			snapshot: snapshotter.SnapshotPlumbing(),
		}
		workers[commitIdx%numWorkers].workChan <- work
		commitIdx++
	}

	return nil
}
