// Package framework provides orchestration for running analysis pipelines.
package framework

import (
	"context"
	"sync"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/burndown"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/couples"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/devs"
	filehistory "github.com/Sumatoshi-tech/codefang/pkg/analyzers/file_history"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/imports"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/plumbing"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/sentiment"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/shotness"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/typos"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
	pkgplumbing "github.com/Sumatoshi-tech/codefang/pkg/plumbing"
	"github.com/Sumatoshi-tech/codefang/pkg/uast"
	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

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
	w := runner.Config.LeafWorkers
	if w > 0 && runner.CoreCount > 0 && runner.CoreCount < len(runner.Analyzers) && !runner.hasSequentialLeaf() {
		return runner.processCommitsParallel(commits, indexOffset)
	}

	return runner.processCommitsSerial(commits, indexOffset)
}

// hasSequentialLeaf returns true if any leaf analyzer requires sequential commit processing
// and cannot be parallelized via round-robin Fork/Merge (e.g., burndown tracks cumulative
// per-file line state across all commits).
func (runner *Runner) hasSequentialLeaf() bool {
	for _, a := range runner.Analyzers[runner.CoreCount:] {
		switch a.(type) {
		case *burndown.HistoryAnalyzer:
			return true // Cumulative per-file line state across all commits.
		case *devs.HistoryAnalyzer:
			return true // Fork() does not isolate mutable map state.
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

// leafWork holds a snapshot of plumbing outputs for one commit,
// allowing leaf analyzers to process it independently.
type leafWork struct {
	ctx         *analyze.Context
	tick        int
	authorID    int
	changes     gitlib.Changes
	blobCache   map[gitlib.Hash]*gitlib.CachedBlob
	fileDiffs   map[string]pkgplumbing.FileDiffData
	uastChanges []uast.Change
	lineStats   map[gitlib.ChangeEntry]pkgplumbing.LineStats
	languages   map[gitlib.Hash]string
}

// leafWorker holds forked plumbing copies and leaf analyzers for one worker goroutine.
type leafWorker struct {
	treeDiff  *plumbing.TreeDiffAnalyzer
	blobCache *plumbing.BlobCacheAnalyzer
	fileDiff  *plumbing.FileDiffAnalyzer
	uast      *plumbing.UASTChangesAnalyzer
	lineStats *plumbing.LinesStatsCalculator
	languages *plumbing.LanguagesDetectionAnalyzer
	ticks     *plumbing.TicksSinceStart
	identity  *plumbing.IdentityDetector

	leaves   []analyze.HistoryAnalyzer
	workChan chan leafWork
}

// applyWork sets the worker's plumbing fields from a work snapshot.
func (w *leafWorker) applyWork(work leafWork) {
	w.treeDiff.Changes = work.changes
	w.blobCache.Cache = work.blobCache
	w.fileDiff.FileDiffs = work.fileDiffs
	w.uast.SetChanges(work.uastChanges)
	w.lineStats.LineStats = work.lineStats
	w.languages.SetLanguages(work.languages)
	w.ticks.Tick = work.tick
	w.identity.AuthorID = work.authorID
}

// processWork applies plumbing snapshot, runs leaf Consume(), then releases UAST trees.
func (w *leafWorker) processWork(work leafWork) error {
	w.applyWork(work)

	for _, leaf := range w.leaves {
		consumeErr := leaf.Consume(work.ctx)
		if consumeErr != nil {
			return consumeErr
		}
	}

	// Release UAST trees — this worker owns them.
	for _, ch := range work.uastChanges {
		node.ReleaseTree(ch.Before)
		node.ReleaseTree(ch.After)
	}

	return nil
}

// findPlumbing extracts typed pointers to plumbing analyzers from the core slice.
type plumbingRefs struct {
	treeDiff  *plumbing.TreeDiffAnalyzer
	blobCache *plumbing.BlobCacheAnalyzer
	fileDiff  *plumbing.FileDiffAnalyzer
	uast      *plumbing.UASTChangesAnalyzer
	lineStats *plumbing.LinesStatsCalculator
	languages *plumbing.LanguagesDetectionAnalyzer
	ticks     *plumbing.TicksSinceStart
	identity  *plumbing.IdentityDetector
}

func findPlumbing(core []analyze.HistoryAnalyzer) plumbingRefs {
	var refs plumbingRefs

	for _, analyzer := range core {
		switch typed := analyzer.(type) {
		case *plumbing.TreeDiffAnalyzer:
			refs.treeDiff = typed
		case *plumbing.BlobCacheAnalyzer:
			refs.blobCache = typed
		case *plumbing.FileDiffAnalyzer:
			refs.fileDiff = typed
		case *plumbing.UASTChangesAnalyzer:
			refs.uast = typed
		case *plumbing.LinesStatsCalculator:
			refs.lineStats = typed
		case *plumbing.LanguagesDetectionAnalyzer:
			refs.languages = typed
		case *plumbing.TicksSinceStart:
			refs.ticks = typed
		case *plumbing.IdentityDetector:
			refs.identity = typed
		}
	}

	return refs
}

// rewireLeaf updates a forked leaf analyzer's plumbing pointers to point to the worker's copies.
func rewireLeaf(leaf analyze.HistoryAnalyzer, worker *leafWorker) {
	switch typed := leaf.(type) {
	case *sentiment.HistoryAnalyzer:
		typed.UAST = worker.uast
		typed.Ticks = worker.ticks
	case *shotness.HistoryAnalyzer:
		typed.UAST = worker.uast
		typed.FileDiff = worker.fileDiff
	case *imports.HistoryAnalyzer:
		typed.TreeDiff = worker.treeDiff
		typed.BlobCache = worker.blobCache
		typed.Identity = worker.identity
		typed.Ticks = worker.ticks
	case *burndown.HistoryAnalyzer:
		typed.Identity = worker.identity
		typed.Ticks = worker.ticks
		typed.BlobCache = worker.blobCache
		typed.FileDiff = worker.fileDiff
		typed.TreeDiff = worker.treeDiff
	case *devs.HistoryAnalyzer:
		typed.Identity = worker.identity
		typed.Ticks = worker.ticks
		typed.TreeDiff = worker.treeDiff
		typed.Languages = worker.languages
		typed.LineStats = worker.lineStats
	case *couples.HistoryAnalyzer:
		typed.Identity = worker.identity
		typed.TreeDiff = worker.treeDiff
	case *typos.HistoryAnalyzer:
		typed.UAST = worker.uast
		typed.BlobCache = worker.blobCache
		typed.FileDiff = worker.fileDiff
	case *filehistory.Analyzer:
		typed.TreeDiff = worker.treeDiff
		typed.LineStats = worker.lineStats
		typed.Identity = worker.identity
	}
}

// newLeafWorkers creates W leaf workers with forked plumbing and leaf analyzers.
func newLeafWorkers(leaves []analyze.HistoryAnalyzer, w int) []*leafWorker {
	workers := make([]*leafWorker, w)

	for i := range w {
		worker := &leafWorker{
			// Create independent plumbing copies (shallow struct copies).
			treeDiff:  &plumbing.TreeDiffAnalyzer{},
			blobCache: &plumbing.BlobCacheAnalyzer{},
			fileDiff:  &plumbing.FileDiffAnalyzer{},
			uast:      &plumbing.UASTChangesAnalyzer{},
			lineStats: &plumbing.LinesStatsCalculator{},
			languages: &plumbing.LanguagesDetectionAnalyzer{},
			ticks:     &plumbing.TicksSinceStart{},
			identity:  &plumbing.IdentityDetector{},
			workChan:  make(chan leafWork, 2),
		}

		// Fork each leaf and rewire to this worker's plumbing.
		worker.leaves = make([]analyze.HistoryAnalyzer, len(leaves))

		for j, leaf := range leaves {
			forked := leaf.Fork(1)
			worker.leaves[j] = forked[0]
			rewireLeaf(worker.leaves[j], worker)
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

// snapshotPlumbing captures the current plumbing outputs into a leafWork struct.
// UAST tree ownership is transferred (cleared from the source analyzer).
func snapshotPlumbing(refs plumbingRefs, analyzeCtx *analyze.Context) leafWork {
	var uastChanges []uast.Change
	if refs.uast != nil {
		uastChanges = refs.uast.TransferChanges()
	}

	work := leafWork{
		ctx:         analyzeCtx,
		tick:        refs.ticks.Tick,
		authorID:    refs.identity.AuthorID,
		changes:     refs.treeDiff.Changes,
		blobCache:   refs.blobCache.Cache,
		fileDiffs:   refs.fileDiff.FileDiffs,
		uastChanges: uastChanges,
	}

	if refs.lineStats != nil {
		work.lineStats = refs.lineStats.LineStats
	}

	if refs.languages != nil {
		work.languages = refs.languages.Languages()
	}

	return work
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
	refs := findPlumbing(core)

	numWorkers := runner.Config.LeafWorkers
	workers := newLeafWorkers(leaves, numWorkers)
	wg, workerErrors := startLeafWorkers(workers)

	// Serial plumbing loop with round-robin dispatch to leaf workers.
	loopErr := runner.consumeAndDispatch(dataChan, core, refs, workers, numWorkers, indexOffset)

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
	refs plumbingRefs,
	workers []*leafWorker,
	numWorkers int,
	indexOffset int,
) error {
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

		work := snapshotPlumbing(refs, analyzeCtx)
		workers[commitIdx%numWorkers].workChan <- work
		commitIdx++
	}

	return nil
}
