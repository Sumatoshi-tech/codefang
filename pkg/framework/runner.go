// Package framework provides orchestration for running analysis pipelines.
package framework

import (
	"context"
	"errors"
	"fmt"
	"sync"
	"time"

	"go.opentelemetry.io/otel"
	"go.opentelemetry.io/otel/attribute"
	"go.opentelemetry.io/otel/trace"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/plumbing"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
	"github.com/Sumatoshi-tech/codefang/pkg/observability"
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

	// Tracer is the OTel tracer for creating pipeline spans.
	// When nil, falls back to otel.Tracer("codefang").
	Tracer trace.Tracer

	// CoreCount is the number of leading analyzers in the Analyzers slice that are
	// core (plumbing) analyzers. These run sequentially. Analyzers after CoreCount
	// are leaf analyzers that can be parallelized via Fork/Merge.
	// Set to 0 to disable parallel leaf consumption.
	CoreCount int

	runtimeTuningOnce sync.Once
	runtimeBallast    []byte
}

// tracerName is the default OTel tracer name for the framework package.
const tracerName = "codefang"

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

// tracer returns the configured tracer, falling back to the global provider.
func (runner *Runner) tracer() trace.Tracer {
	if runner.Tracer != nil {
		return runner.Tracer
	}

	return otel.Tracer(tracerName)
}

// Run executes all analyzers over the given commits: initialize, consume each commit via pipeline, then finalize.
func (runner *Runner) Run(ctx context.Context, commits []*gitlib.Commit) (map[analyze.HistoryAnalyzer]analyze.Report, error) {
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

	return runner.runInternal(ctx, commits)
}

// runInternal uses the Coordinator to process commits (batch blob + batch diff in C), then feeds analyzers.
func (runner *Runner) runInternal(ctx context.Context, commits []*gitlib.Commit) (map[analyze.HistoryAnalyzer]analyze.Report, error) {
	_, processErr := runner.processCommits(ctx, commits, 0, 0)
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
// chunkIndex is the zero-based chunk number used for span naming.
func (runner *Runner) ProcessChunk(ctx context.Context, commits []*gitlib.Commit, indexOffset, chunkIndex int) (PipelineStats, error) {
	return runner.processCommits(ctx, commits, indexOffset, chunkIndex)
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

// ProcessChunkFromData consumes pre-fetched CommitData through analyzers,
// bypassing Coordinator creation. Used by double-buffered chunk pipelining
// where the pipeline has already run and collected data.
// Returns zero PipelineStats since the real stats come from the prefetch Coordinator.
func (runner *Runner) ProcessChunkFromData(ctx context.Context, data []CommitData, indexOffset, chunkIndex int) (PipelineStats, error) {
	ctx, span := runner.tracer().Start(ctx, "codefang.chunk",
		trace.WithAttributes(
			attribute.Int("chunk.index", chunkIndex),
			attribute.Int("chunk.size", len(data)),
			attribute.Int("chunk.offset", indexOffset),
		))

	runner.runtimeTuningOnce.Do(func() {
		runner.runtimeBallast = applyRuntimeTuning(runner.Config)
	})

	analyzerDurations := make([]time.Duration, len(runner.Analyzers))

	for _, cd := range data {
		if cd.Error != nil {
			observability.RecordSpanError(span, cd.Error, observability.ErrTypeDependencyUnavailable, observability.ErrSourceDependency)
			span.End()

			return PipelineStats{}, cd.Error
		}

		analyzeCtx := buildAnalyzeContext(cd, indexOffset)

		for i, a := range runner.Analyzers {
			start := time.Now()

			err := a.Consume(ctx, analyzeCtx)

			analyzerDurations[i] += time.Since(start)

			if err != nil {
				observability.RecordSpanError(span, err, observability.ErrTypeInternal, observability.ErrSourceServer)
				span.End()

				return PipelineStats{}, err
			}
		}
	}

	span.End()
	runner.emitAnalyzerSpans(ctx, analyzerDurations)

	return PipelineStats{}, nil
}

// processCommits processes commits through the pipeline without Initialize/Finalize.
func (runner *Runner) processCommits(ctx context.Context, commits []*gitlib.Commit, indexOffset, chunkIndex int) (PipelineStats, error) {
	runner.runtimeTuningOnce.Do(func() {
		runner.runtimeBallast = applyRuntimeTuning(runner.Config)
	})

	w := runner.Config.LeafWorkers
	if w > 0 && runner.CoreCount > 0 && runner.CoreCount < len(runner.Analyzers) {
		cpuHeavy, lightweight, serialLeaves := runner.splitLeaves()
		if len(cpuHeavy) > 0 {
			return runner.processCommitsHybrid(ctx, commits, indexOffset, chunkIndex, cpuHeavy, lightweight, serialLeaves)
		}
	}

	return runner.processCommitsSerial(ctx, commits, indexOffset, chunkIndex)
}

// splitLeaves partitions leaf analyzers into three groups:
//   - cpuHeavy: Parallelizable, not SequentialOnly, CPUHeavy — dispatched to W workers via Fork/Merge.
//   - lightweight: Parallelizable, not SequentialOnly, not CPUHeavy — run on main goroutine.
//   - serial: SequentialOnly or not Parallelizable — run on main goroutine.
func (runner *Runner) splitLeaves() (cpuHeavy, lightweight, serial []analyze.HistoryAnalyzer) {
	for _, leaf := range runner.Analyzers[runner.CoreCount:] {
		par, ok := leaf.(analyze.Parallelizable)
		if !ok || par.SequentialOnly() {
			serial = append(serial, leaf)

			continue
		}

		if par.CPUHeavy() {
			cpuHeavy = append(cpuHeavy, leaf)
		} else {
			lightweight = append(lightweight, leaf)
		}
	}

	return cpuHeavy, lightweight, serial
}

// processCommitsSerial is the original serial consumption path.
func (runner *Runner) processCommitsSerial(
	ctx context.Context, commits []*gitlib.Commit, indexOffset, chunkIndex int,
) (PipelineStats, error) {
	ctx, span := runner.tracer().Start(ctx, "codefang.chunk",
		trace.WithAttributes(
			attribute.Int("chunk.index", chunkIndex),
			attribute.Int("chunk.size", len(commits)),
			attribute.Int("chunk.offset", indexOffset),
		))

	coordinator := NewCoordinator(runner.Repo, runner.Config)
	dataChan := coordinator.Process(ctx, commits)

	analyzerDurations := make([]time.Duration, len(runner.Analyzers))

	for data := range dataChan {
		if data.Error != nil {
			observability.RecordSpanError(span, data.Error, observability.ErrTypeDependencyUnavailable, observability.ErrSourceDependency)
			span.End()

			return PipelineStats{}, data.Error
		}

		analyzeCtx := buildAnalyzeContext(data, indexOffset)

		for i, a := range runner.Analyzers {
			start := time.Now()

			err := a.Consume(ctx, analyzeCtx)

			analyzerDurations[i] += time.Since(start)

			if err != nil {
				observability.RecordSpanError(span, err, observability.ErrTypeInternal, observability.ErrSourceServer)
				span.End()

				return PipelineStats{}, err
			}
		}
	}

	pStats := coordinator.Stats()
	setPipelineAttributes(span, pStats)
	span.End()
	runner.emitAnalyzerSpans(ctx, analyzerDurations)

	return pStats, nil
}

// leafWork holds an opaque plumbing snapshot and analyze context for one commit.
type leafWork struct {
	analyzeCtx *analyze.Context
	snapshot   analyze.PlumbingSnapshot
}

// leafWorker holds forked leaf analyzers for one worker goroutine.
type leafWorker struct {
	leaves    []analyze.HistoryAnalyzer
	workChan  chan leafWork
	durations []time.Duration // Accumulated per-leaf-analyzer durations.
}

// processWork applies the plumbing snapshot, runs leaf Consume(), then releases snapshot resources.
func (w *leafWorker) processWork(ctx context.Context, work leafWork) error {
	for i, leaf := range w.leaves {
		p, ok := leaf.(analyze.Parallelizable)
		if !ok {
			return fmt.Errorf("%w: %s", ErrNotParallelizable, leaf.Name())
		}

		p.ApplySnapshot(work.snapshot)

		start := time.Now()

		consumeErr := leaf.Consume(ctx, work.analyzeCtx)

		w.durations[i] += time.Since(start)

		if consumeErr != nil {
			return consumeErr
		}
	}

	// Release snapshot resources (e.g. UAST trees).
	releaseSnapshot(work.snapshot)

	return nil
}

// newLeafWorkers creates W leaf workers with forked leaf analyzers.
// Each forked leaf owns independent plumbing struct copies (created by Fork()).
func newLeafWorkers(leaves []analyze.HistoryAnalyzer, w int) []*leafWorker {
	// leafWorkChanBuffer is the channel buffer size for each leaf worker.
	// A small buffer allows one commit to be queued while another is being processed.
	const leafWorkChanBuffer = 2

	workers := make([]*leafWorker, w)

	for i := range w {
		worker := &leafWorker{
			workChan:  make(chan leafWork, leafWorkChanBuffer),
			durations: make([]time.Duration, len(leaves)),
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
func startLeafWorkers(ctx context.Context, workers []*leafWorker) (*sync.WaitGroup, []error) {
	numWorkers := len(workers)

	var wg sync.WaitGroup

	workerErrors := make([]error, numWorkers)

	wg.Add(numWorkers)

	for idx, wk := range workers {
		go func(workerIdx int, worker *leafWorker) {
			defer wg.Done()

			for work := range worker.workChan {
				processErr := worker.processWork(ctx, work)
				if processErr != nil {
					workerErrors[workerIdx] = processErr

					// Drain remaining work to prevent deadlock.
					for range worker.workChan {
						continue
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

// releaseSnapshot releases resources owned by a plumbing snapshot.
// Called once per snapshot after all leaves in the worker have consumed it.
func releaseSnapshot(snap analyze.PlumbingSnapshot) {
	s, ok := snap.(plumbing.Snapshot)
	if !ok {
		return
	}

	plumbing.ReleaseSnapshotUAST(s)
}

// buildCompositeSnapshot calls SnapshotPlumbing on every parallel leaf and
// merges the results into a single plumbing.Snapshot. Each leaf only populates
// the fields it depends on, so we take the first non-zero value for each field.
// This ensures that all plumbing data (including UAST changes) is captured
// regardless of which leaf happens to be first in the slice.
func buildCompositeSnapshot(snapshotters []analyze.Parallelizable) analyze.PlumbingSnapshot {
	var composite plumbing.Snapshot

	for _, s := range snapshotters {
		snap, ok := s.SnapshotPlumbing().(plumbing.Snapshot)
		if !ok {
			continue
		}

		mergeSnapshotMaps(&composite, snap)
		mergeSnapshotScalars(&composite, snap)
	}

	return composite
}

// mergeSnapshotMaps copies nil reference-type fields (slices and maps) from snap
// into composite, taking the first non-nil value for each field.
func mergeSnapshotMaps(composite *plumbing.Snapshot, snap plumbing.Snapshot) {
	if composite.Changes == nil && snap.Changes != nil {
		composite.Changes = snap.Changes
	}

	if composite.BlobCache == nil && snap.BlobCache != nil {
		composite.BlobCache = snap.BlobCache
	}

	if composite.FileDiffs == nil && snap.FileDiffs != nil {
		composite.FileDiffs = snap.FileDiffs
	}

	if composite.LineStats == nil && snap.LineStats != nil {
		composite.LineStats = snap.LineStats
	}

	if composite.Languages == nil && snap.Languages != nil {
		composite.Languages = snap.Languages
	}

	if composite.UASTChanges == nil && snap.UASTChanges != nil {
		composite.UASTChanges = snap.UASTChanges
	}
}

// mergeSnapshotScalars copies zero-valued scalar fields from snap into composite,
// taking the first non-zero value for each field.
func mergeSnapshotScalars(composite *plumbing.Snapshot, snap plumbing.Snapshot) {
	if composite.Tick == 0 && snap.Tick != 0 {
		composite.Tick = snap.Tick
	}

	if composite.AuthorID == 0 && snap.AuthorID != 0 {
		composite.AuthorID = snap.AuthorID
	}
}

// closeWorkersAndWait closes all worker channels and waits for goroutines to finish.
func closeWorkersAndWait(workers []*leafWorker, wg *sync.WaitGroup) {
	for _, worker := range workers {
		close(worker.workChan)
	}

	wg.Wait()
}

// collectSnapshotters extracts Parallelizable interfaces from leaf analyzers.
func collectSnapshotters(leaves []analyze.HistoryAnalyzer) ([]analyze.Parallelizable, error) {
	snapshotters := make([]analyze.Parallelizable, 0, len(leaves))

	for _, leaf := range leaves {
		p, ok := leaf.(analyze.Parallelizable)
		if !ok {
			return nil, fmt.Errorf("%w: %s", ErrNotParallelizable, leaf.Name())
		}

		snapshotters = append(snapshotters, p)
	}

	return snapshotters, nil
}

// buildAnalyzeContext creates an analyze.Context from pipeline commit data.
func buildAnalyzeContext(data CommitData, indexOffset int) *analyze.Context {
	commit := data.Commit

	return &analyze.Context{
		Commit:      commit,
		Index:       data.Index + indexOffset,
		Time:        commit.Committer().When,
		IsMerge:     commit.NumParents() > 1,
		Changes:     data.Changes,
		BlobCache:   data.BlobCache,
		FileDiffs:   data.FileDiffs,
		UASTChanges: data.UASTChanges,
	}
}

// processCommitsHybrid processes commits with taxonomy-aware dispatch:
//   - core analyzers run sequentially on the main goroutine.
//   - cpuHeavy leaves are dispatched to W workers via Fork/Merge.
//   - lightweight and serial leaves run on the main goroutine.
func (runner *Runner) processCommitsHybrid(
	ctx context.Context,
	commits []*gitlib.Commit,
	indexOffset, chunkIndex int,
	cpuHeavy, lightweight, serialLeaves []analyze.HistoryAnalyzer,
) (PipelineStats, error) {
	ctx, span := runner.tracer().Start(ctx, "codefang.chunk",
		trace.WithAttributes(
			attribute.Int("chunk.index", chunkIndex),
			attribute.Int("chunk.size", len(commits)),
			attribute.Int("chunk.offset", indexOffset),
		))

	coordinator := NewCoordinator(runner.Repo, runner.Config)
	dataChan := coordinator.Process(ctx, commits)

	core := runner.Analyzers[:runner.CoreCount]

	numWorkers := runner.Config.LeafWorkers
	workers := newLeafWorkers(cpuHeavy, numWorkers)
	wg, workerErrors := startLeafWorkers(ctx, workers)

	// Collect snapshotters from both cpuHeavy and lightweight leaves so the
	// composite snapshot captures all plumbing fields (e.g. UASTChanges from
	// cpuHeavy, Changes/BlobCache from lightweight).
	allParallel := make([]analyze.HistoryAnalyzer, 0, len(cpuHeavy)+len(lightweight))
	allParallel = append(allParallel, cpuHeavy...)
	allParallel = append(allParallel, lightweight...)

	snapshotters, snapErr := collectSnapshotters(allParallel)
	if snapErr != nil {
		span.End()

		return PipelineStats{}, snapErr
	}

	// Combine lightweight and serial leaves into one main-goroutine group.
	mainLeaves := make([]analyze.HistoryAnalyzer, 0, len(lightweight)+len(serialLeaves))
	mainLeaves = append(mainLeaves, lightweight...)
	mainLeaves = append(mainLeaves, serialLeaves...)

	coreDurations, mainDurations, loopErr := runner.hybridCommitLoop(
		ctx, dataChan, indexOffset, core, mainLeaves, snapshotters, workers, numWorkers, wg)
	if loopErr != nil {
		span.End()

		return PipelineStats{}, loopErr
	}

	for _, workerErr := range workerErrors {
		if workerErr != nil {
			span.End()

			return PipelineStats{}, workerErr
		}
	}

	mergeLeafResults(cpuHeavy, workers)

	pStats := coordinator.Stats()
	setPipelineAttributes(span, pStats)
	span.End()

	// Core durations are not emitted as spans (infrastructure).
	_ = coreDurations

	// Emit per-analyzer spans for leaf analyzers.
	runner.emitHybridAnalyzerSpans(ctx, mainLeaves, mainDurations, cpuHeavy, workers)

	return pStats, nil
}

// hybridCommitLoop iterates over pipeline data, dispatching work to parallel workers
// and running core/serial analyzers on the main goroutine.
// Returns accumulated durations for core and main-goroutine leaf analyzers.
func (runner *Runner) hybridCommitLoop(
	ctx context.Context,
	dataChan <-chan CommitData,
	indexOffset int,
	core, serialLeaves []analyze.HistoryAnalyzer,
	snapshotters []analyze.Parallelizable,
	workers []*leafWorker,
	numWorkers int,
	wg *sync.WaitGroup,
) (coreDurations, mainDurations []time.Duration, err error) {
	coreDurations = make([]time.Duration, len(core))
	mainDurations = make([]time.Duration, len(serialLeaves))

	var commitIdx int

	for data := range dataChan {
		if data.Error != nil {
			closeWorkersAndWait(workers, wg)

			return nil, nil, data.Error
		}

		analyzeCtx := buildAnalyzeContext(data, indexOffset)

		// Run core (plumbing) analyzers sequentially.
		for i, a := range core {
			start := time.Now()

			coreErr := a.Consume(ctx, analyzeCtx)

			coreDurations[i] += time.Since(start)

			if coreErr != nil {
				closeWorkersAndWait(workers, wg)

				return nil, nil, coreErr
			}
		}

		// Snapshot plumbing state for parallel workers before serial leaves mutate anything.
		// Build a composite snapshot from ALL parallel leaves so every plumbing field
		// (Changes, BlobCache, FileDiffs, UAST, Tick, AuthorID, etc.) is captured.
		work := leafWork{
			analyzeCtx: analyzeCtx,
			snapshot:   buildCompositeSnapshot(snapshotters),
		}

		// Dispatch parallel leaves to a worker.
		workers[commitIdx%numWorkers].workChan <- work

		// Run serial leaves on the main goroutine.
		for i, a := range serialLeaves {
			start := time.Now()

			leafErr := a.Consume(ctx, analyzeCtx)

			mainDurations[i] += time.Since(start)

			if leafErr != nil {
				closeWorkersAndWait(workers, wg)

				return nil, nil, leafErr
			}
		}

		commitIdx++
	}

	// Close all work channels to signal workers to finish.
	closeWorkersAndWait(workers, wg)

	return coreDurations, mainDurations, nil
}

// setPipelineAttributes sets pipeline timing and cache stats as attributes on a chunk span.
func setPipelineAttributes(span trace.Span, ps PipelineStats) {
	span.SetAttributes(
		attribute.Int64("pipeline.blob_ms", ps.BlobDuration.Milliseconds()),
		attribute.Int64("pipeline.diff_ms", ps.DiffDuration.Milliseconds()),
		attribute.Int64("pipeline.uast_ms", ps.UASTDuration.Milliseconds()),
		attribute.Int64("cache.blob.hits", ps.BlobCacheHits),
		attribute.Int64("cache.blob.misses", ps.BlobCacheMisses),
		attribute.Int64("cache.diff.hits", ps.DiffCacheHits),
		attribute.Int64("cache.diff.misses", ps.DiffCacheMisses),
	)
}

// emitAnalyzerSpans creates per-analyzer child spans with accumulated durations.
// Only leaf analyzers (index >= CoreCount) get spans; core analyzers are infrastructure.
func (runner *Runner) emitAnalyzerSpans(ctx context.Context, durations []time.Duration) {
	tr := runner.tracer()
	now := time.Now()

	for i, a := range runner.Analyzers {
		if i < runner.CoreCount {
			continue
		}

		_, aSpan := tr.Start(ctx, "codefang.analyzer."+a.Name(),
			trace.WithTimestamp(now.Add(-durations[i])))
		aSpan.End(trace.WithTimestamp(now))
	}
}

// emitHybridAnalyzerSpans creates per-analyzer spans for hybrid mode where
// main-goroutine leaves and worker leaves have separate duration tracking.
func (runner *Runner) emitHybridAnalyzerSpans(
	ctx context.Context,
	mainLeaves []analyze.HistoryAnalyzer, mainDurations []time.Duration,
	cpuHeavy []analyze.HistoryAnalyzer, workers []*leafWorker,
) {
	tr := runner.tracer()
	now := time.Now()

	// Main-goroutine leaves (lightweight + serial).
	for i, leaf := range mainLeaves {
		_, aSpan := tr.Start(ctx, "codefang.analyzer."+leaf.Name(),
			trace.WithTimestamp(now.Add(-mainDurations[i])))
		aSpan.End(trace.WithTimestamp(now))
	}

	// CPU-heavy leaves: sum durations across all workers.
	for leafIdx, leaf := range cpuHeavy {
		var total time.Duration

		for _, worker := range workers {
			total += worker.durations[leafIdx]
		}

		_, aSpan := tr.Start(ctx, "codefang.analyzer."+leaf.Name(),
			trace.WithTimestamp(now.Add(-total)))
		aSpan.End(trace.WithTimestamp(now))
	}
}
