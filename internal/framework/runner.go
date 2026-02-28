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

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/plumbing"
	"github.com/Sumatoshi-tech/codefang/internal/checkpoint"
	"github.com/Sumatoshi-tech/codefang/internal/observability"
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

	// Tracer is the OTel tracer for creating pipeline spans.
	// When nil, falls back to otel.Tracer("codefang").
	Tracer trace.Tracer

	// CoreCount is the number of leading analyzers in the Analyzers slice that are
	// core (plumbing) analyzers. These run sequentially. Analyzers after CoreCount
	// are leaf analyzers that can be parallelized via Fork/Merge.
	// Set to 0 to disable parallel leaf consumption.
	CoreCount int

	// MemBudget is the user's memory budget in bytes. When positive, overrides
	// the system-RAM-based debug.SetMemoryLimit with a budget-aligned value.
	MemBudget int64

	// aggregators holds one aggregator per analyzer slot (indexed same as Analyzers).
	// nil for core analyzers and leaf analyzers without aggregators (e.g. file_history).
	aggregators []analyze.Aggregator

	// tickProvider and idProvider are discovered from core analyzers during initAggregators.
	// Used to stamp TCs with tick/author metadata before feeding to aggregators.
	tickProvider *plumbing.TicksSinceStart
	idProvider   *plumbing.IdentityDetector

	// commitMeta accumulates per-commit metadata (timestamp, author) during TC consumption.
	// Injected into Reports by FinalizeWithAggregators for timeseries output.
	commitMeta map[string]analyze.CommitMeta

	// TCSink, when set, receives every non-nil TC as commits are consumed.
	// Used by NDJSON streaming output. When set, aggregators are not created
	// and FinalizeWithAggregators is not called.
	TCSink analyze.TCSink

	// AggSpillBudget is the maximum bytes of aggregator state to keep in memory
	// before spilling to disk. Computed by ComputeSchedule from the memory budget.
	// Zero means no limit (unlimited budget or budget too small to decompose).
	AggSpillBudget int64

	// tcBytesAccumulated tracks total TC payload bytes consumed since last reset.
	// Used by three-metric adaptive feedback to measure TC size per commit.
	tcBytesAccumulated int64

	runtimeTuningOnce sync.Once
	runtimeBallast    []byte
}

// tracerName is the default OTel tracer name for the framework package.
const tracerName = "codefang"

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

	runner.initAggregators()

	if len(commits) == 0 {
		return runner.FinalizeWithAggregators(ctx)
	}

	_, processErr := runner.processCommits(ctx, commits, 0, 0)
	if processErr != nil {
		return nil, processErr
	}

	return runner.FinalizeWithAggregators(ctx)
}

// Initialize initializes all analyzers and creates aggregators.
// Call once before processing chunks.
func (runner *Runner) Initialize() error {
	for _, a := range runner.Analyzers {
		err := a.Initialize(runner.Repo)
		if err != nil {
			return err
		}
	}

	runner.initAggregators()

	return nil
}

// initAggregators creates aggregators for leaf analyzers and discovers
// plumbing providers (tick + identity) from core analyzers.
// Called once after all analyzers are initialized.
func (runner *Runner) initAggregators() {
	runner.aggregators = make([]analyze.Aggregator, len(runner.Analyzers))
	runner.commitMeta = make(map[string]analyze.CommitMeta)

	for i, a := range runner.Analyzers {
		if i < runner.CoreCount {
			// Discover plumbing providers from core analyzers.
			if tp, ok := a.(*plumbing.TicksSinceStart); ok {
				runner.tickProvider = tp
			}

			if id, ok := a.(*plumbing.IdentityDetector); ok {
				runner.idProvider = id
			}

			continue
		}

		// When TCSink is set (NDJSON mode), TCs go directly to the sink
		// and aggregators are not needed.
		if runner.TCSink != nil {
			continue
		}

		agg := a.NewAggregator(analyze.AggregatorOptions{
			SpillBudget: runner.AggSpillBudget,
		})
		runner.aggregators[i] = agg // nil for analyzers without aggregators.
	}
}

// InitializeForResume initializes all analyzers and recreates aggregators
// with saved spill state from a checkpoint. Called instead of Initialize()
// when resuming from a checkpoint (startChunk > 0).
func (runner *Runner) InitializeForResume(aggSpills []checkpoint.AggregatorSpillEntry) error {
	for _, a := range runner.Analyzers {
		err := a.Initialize(runner.Repo)
		if err != nil {
			return err
		}
	}

	runner.initAggregators()

	// Restore spill state for aggregators that have saved spill dirs.
	for i, agg := range runner.aggregators {
		if agg == nil || i >= len(aggSpills) {
			continue
		}

		entry := aggSpills[i]
		if entry.Dir == "" {
			continue
		}

		agg.RestoreSpillState(analyze.AggregatorSpillInfo{
			Dir:   entry.Dir,
			Count: entry.Count,
		})
	}

	return nil
}

// AggregatorSpills returns the current spill state of all aggregators
// for checkpoint persistence.
func (runner *Runner) AggregatorSpills() []checkpoint.AggregatorSpillEntry {
	spills := make([]checkpoint.AggregatorSpillEntry, len(runner.aggregators))

	for i, agg := range runner.aggregators {
		if agg == nil {
			continue
		}

		info := agg.SpillState()
		spills[i] = checkpoint.AggregatorSpillEntry{
			Dir:   info.Dir,
			Count: info.Count,
		}
	}

	return spills
}

// SpillAggregators forces all aggregators to flush their in-memory state
// to disk. Called before saving a checkpoint so that spill files are complete.
func (runner *Runner) SpillAggregators() error {
	for _, agg := range runner.aggregators {
		if agg == nil {
			continue
		}

		_, err := agg.Spill()
		if err != nil {
			return err
		}
	}

	return nil
}

// DiscardAggregatorState clears all in-memory cumulative state from
// aggregators without serialization. Used in streaming timeseries NDJSON
// mode where per-commit data is drained each chunk and cumulative state
// (coupling matrices, burndown histories) is never needed for a final report.
func (runner *Runner) DiscardAggregatorState() {
	for _, agg := range runner.aggregators {
		if agg == nil {
			continue
		}

		if d, ok := agg.(analyze.StateDiscarder); ok {
			d.DiscardState()
		}
	}
}

// DiscardLeafAnalyzerState clears cumulative state from leaf history analyzers
// that implement analyze.StateDiscarder. This complements DiscardAggregatorState
// by freeing state held directly in analyzers (e.g. shotness node coupling maps)
// rather than in aggregators.
func (runner *Runner) DiscardLeafAnalyzerState() {
	for _, leaf := range runner.LeafAnalyzers() {
		if d, ok := leaf.(analyze.StateDiscarder); ok {
			d.DiscardState()
		}
	}
}

// LeafAnalyzers returns the history analyzers registered as leaves (non-plumbing).
func (runner *Runner) LeafAnalyzers() []analyze.HistoryAnalyzer {
	if runner.CoreCount >= len(runner.Analyzers) {
		return nil
	}

	return runner.Analyzers[runner.CoreCount:]
}

// addTC stamps a TC with tick/author/timestamp metadata and routes it to
// either the TCSink (NDJSON mode) or the corresponding aggregator.
// Skips TCs with nil Data.
func (runner *Runner) addTC(tc analyze.TC, idx int, ac *analyze.Context) {
	if tc.Data == nil {
		return
	}

	if runner.tickProvider != nil {
		tc.Tick = runner.tickProvider.Tick
	}

	if runner.idProvider != nil {
		tc.AuthorID = runner.idProvider.AuthorID
	}

	tc.Timestamp = ac.Time
	runner.recordCommitMeta(tc)

	if runner.TCSink != nil {
		runner.sendToSink(tc, idx)

		return
	}

	if runner.aggregators[idx] == nil {
		return
	}

	// Track TC count for per-chunk metrics.
	runner.tcBytesAccumulated++

	addErr := runner.aggregators[idx].Add(tc)
	if addErr != nil {
		// Add errors are programming errors (type mismatch). Log is not available
		// here, so we silently discard. The aggregator remains in a valid state.
		return
	}
}

// AggregatorStateSize returns the sum of EstimatedStateSize() across all
// non-nil aggregators. Used for three-metric adaptive feedback.
func (runner *Runner) AggregatorStateSize() int64 {
	var total int64

	for _, agg := range runner.aggregators {
		if agg != nil {
			total += agg.EstimatedStateSize()
		}
	}

	return total
}

// TCCountAccumulated returns the number of TCs added since the last reset.
func (runner *Runner) TCCountAccumulated() int64 {
	return runner.tcBytesAccumulated
}

// ResetTCCount resets the per-chunk TC counter.
func (runner *Runner) ResetTCCount() {
	runner.tcBytesAccumulated = 0
}

// recordCommitMeta stores per-commit metadata from a stamped TC.
// Deduplicates by commit hash — the same commit produces identical metadata
// regardless of which analyzer emitted the TC.
func (runner *Runner) recordCommitMeta(tc analyze.TC) {
	hashStr := tc.CommitHash.String()
	if _, exists := runner.commitMeta[hashStr]; exists {
		return
	}

	var ts string
	if !tc.Timestamp.IsZero() {
		ts = tc.Timestamp.Format(time.RFC3339)
	}

	runner.commitMeta[hashStr] = analyze.CommitMeta{
		Hash:      hashStr,
		Tick:      tc.Tick,
		Timestamp: ts,
		Author:    runner.authorName(tc.AuthorID),
	}
}

// authorName resolves an AuthorID to a human-readable name via ReversedPeopleDict.
// Returns empty string when no identity detector is configured or AuthorID is out of bounds.
func (runner *Runner) authorName(authorID int) string {
	if runner.idProvider == nil {
		return ""
	}

	dict := runner.idProvider.ReversedPeopleDict
	if authorID < 0 || authorID >= len(dict) {
		return ""
	}

	return dict[authorID]
}

// analyzerIndex builds a reverse map from analyzer to its index in runner.Analyzers.
func (runner *Runner) analyzerIndex() map[analyze.HistoryAnalyzer]int {
	m := make(map[analyze.HistoryAnalyzer]int, len(runner.Analyzers))
	for i, a := range runner.Analyzers {
		m[a] = i
	}

	return m
}

// drainWorkerTCs feeds buffered TCs from parallel workers into aggregators or TCSink.
func (runner *Runner) drainWorkerTCs(workers []*leafWorker) {
	tcsByIdx := make([][]bufferedTC, len(runner.Analyzers))

	for _, worker := range workers {
		for _, btc := range worker.tcs {
			runner.recordCommitMeta(btc.tc)
			tcsByIdx[btc.idx] = append(tcsByIdx[btc.idx], btc)
		}
	}

	var wg sync.WaitGroup

	for _, tcs := range tcsByIdx {
		if len(tcs) == 0 {
			continue
		}

		wg.Add(1)

		go func(tcs []bufferedTC) {
			defer wg.Done()

			for _, btc := range tcs {
				runner.routeBufferedTC(btc)
			}
		}(tcs)
	}

	wg.Wait()
}

// sendToSink dispatches a TC to the TCSink callback. Errors are silently
// discarded — sink failures (e.g. broken pipe) should not halt the pipeline.
func (runner *Runner) sendToSink(tc analyze.TC, idx int) {
	sinkErr := runner.TCSink(tc, runner.Analyzers[idx].Flag())
	if sinkErr != nil {
		return
	}
}

// routeBufferedTC sends a single buffered TC to the TCSink or its aggregator.
func (runner *Runner) routeBufferedTC(btc bufferedTC) {
	if runner.TCSink != nil {
		runner.sendToSink(btc.tc, btc.idx)

		return
	}

	agg := runner.aggregators[btc.idx]
	if agg == nil {
		return
	}

	addErr := agg.Add(btc.tc)
	if addErr != nil {
		return
	}
}

// mapIndices returns original runner.Analyzers indices for the given leaf analyzers.
func mapIndices(leaves []analyze.HistoryAnalyzer, idxMap map[analyze.HistoryAnalyzer]int) []int {
	indices := make([]int, len(leaves))
	for i, a := range leaves {
		indices[i] = idxMap[a]
	}

	return indices
}

// consumeAll feeds one commit through all analyzers, accumulating per-analyzer durations.
func (runner *Runner) consumeAll(ctx context.Context, ac *analyze.Context, durations []time.Duration) error {
	for i, a := range runner.Analyzers {
		start := time.Now()

		tc, err := a.Consume(ctx, ac)

		durations[i] += time.Since(start)

		if err != nil {
			return err
		}

		runner.addTC(tc, i, ac)
	}

	return nil
}

// buildLeafWork creates a leafWork with plumbing snapshot and TC stamping metadata.
func (runner *Runner) buildLeafWork(ac *analyze.Context, snapshotters []analyze.Parallelizable) leafWork {
	var tick, authorID int

	if runner.tickProvider != nil {
		tick = runner.tickProvider.Tick
	}

	if runner.idProvider != nil {
		authorID = runner.idProvider.AuthorID
	}

	return leafWork{
		analyzeCtx: ac,
		snapshot:   buildCompositeSnapshot(snapshotters),
		tick:       tick,
		authorID:   authorID,
		timestamp:  ac.Time,
	}
}

// closeAggregators releases all aggregator resources.
func (runner *Runner) closeAggregators() {
	for _, agg := range runner.aggregators {
		if agg != nil {
			_ = agg.Close()
		}
	}
}

// ProcessChunk processes a chunk of commits without Initialize/Finalize.
// Use this for streaming mode where Initialize is called once at start
// and Finalize once at end.
// The indexOffset is added to the commit index to maintain correct ordering across chunks.
// chunkIndex is the zero-based chunk number used for span naming.
func (runner *Runner) ProcessChunk(ctx context.Context, commits []*gitlib.Commit, indexOffset, chunkIndex int) (PipelineStats, error) {
	return runner.processCommits(ctx, commits, indexOffset, chunkIndex)
}

// reportFromAggregator collects, flushes, and converts aggregated TICKs to a report.
func reportFromAggregator(ctx context.Context, agg analyze.Aggregator, a analyze.HistoryAnalyzer) (analyze.Report, error) {
	collectErr := agg.Collect()
	if collectErr != nil {
		return nil, fmt.Errorf("collect %s: %w", a.Name(), collectErr)
	}

	ticks, flushErr := agg.FlushAllTicks()
	if flushErr != nil {
		return nil, fmt.Errorf("flush %s: %w", a.Name(), flushErr)
	}

	rep, repErr := a.ReportFromTICKs(ctx, ticks)
	if repErr != nil {
		return nil, fmt.Errorf("report %s: %w", a.Name(), repErr)
	}

	return rep, nil
}

// FinalizeWithAggregators produces reports from all leaf analyzers:
//   - Analyzers with aggregators: Collect → FlushAllTicks → ReportFromTICKs
//   - Analyzers without aggregators: store empty report.
//
// Closes all aggregators before returning.
func (runner *Runner) FinalizeWithAggregators(ctx context.Context) (map[analyze.HistoryAnalyzer]analyze.Report, error) {
	defer runner.closeAggregators()

	if runner.idProvider != nil {
		runner.idProvider.FinalizeDict()
	}

	reports := make(map[analyze.HistoryAnalyzer]analyze.Report, len(runner.Analyzers))

	for i, a := range runner.Analyzers {
		if i < runner.CoreCount {
			continue
		}

		agg := runner.aggregators[i]

		if agg == nil {
			// Plumbing analyzers and analyzers without aggregators store empty report.
			reports[a] = analyze.Report{}

			continue
		}

		rep, err := reportFromAggregator(ctx, agg, a)
		if err != nil {
			return nil, err
		}

		if rep != nil && runner.idProvider != nil && len(runner.idProvider.ReversedPeopleDict) > 0 {
			rep["ReversedPeopleDict"] = runner.idProvider.ReversedPeopleDict
		}

		reports[a] = rep
	}

	runner.injectCommitMeta(reports)

	return reports, nil
}

// injectCommitMeta adds the accumulated commit metadata into each Report
// that contains a "commits_by_tick" key. This enables the timeseries output
// path to populate CommitMeta.Timestamp and CommitMeta.Author.
func (runner *Runner) injectCommitMeta(reports map[analyze.HistoryAnalyzer]analyze.Report) {
	if len(runner.commitMeta) == 0 {
		return
	}

	for _, report := range reports {
		if report == nil {
			continue
		}

		if _, hasCBT := report["commits_by_tick"]; hasCBT {
			report[analyze.ReportKeyCommitMeta] = runner.commitMeta
		}
	}
}

// DrainCommitMeta returns the accumulated per-commit metadata and resets the map.
// Used by streaming timeseries NDJSON to extract metadata between chunks.
func (runner *Runner) DrainCommitMeta() map[string]analyze.CommitMeta {
	meta := runner.commitMeta
	runner.commitMeta = make(map[string]analyze.CommitMeta)

	return meta
}

// LeafAggregators returns the aggregators for leaf analyzers (indices >= CoreCount).
// Used by streaming timeseries NDJSON to drain per-commit data between chunks.
func (runner *Runner) LeafAggregators() []analyze.Aggregator {
	if runner.CoreCount >= len(runner.aggregators) {
		return nil
	}

	return runner.aggregators[runner.CoreCount:]
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
		runner.runtimeBallast = applyRuntimeTuning(runner.Config, runner.MemBudget)
	})

	analyzerDurations := make([]time.Duration, len(runner.Analyzers))

	for _, cd := range data {
		if cd.Error != nil {
			observability.RecordSpanError(span, cd.Error, observability.ErrTypeDependencyUnavailable, observability.ErrSourceDependency)
			span.End()

			return PipelineStats{}, cd.Error
		}

		analyzeCtx := runner.buildAnalyzeContext(cd, indexOffset)

		consumeErr := runner.consumeAll(ctx, analyzeCtx, analyzerDurations)
		if consumeErr != nil {
			observability.RecordSpanError(span, consumeErr, observability.ErrTypeInternal, observability.ErrSourceServer)
			span.End()

			return PipelineStats{}, consumeErr
		}
	}

	span.End()
	runner.emitAnalyzerSpans(ctx, analyzerDurations)

	return PipelineStats{}, nil
}

// processCommits processes commits through the pipeline without Initialize/Finalize.
func (runner *Runner) processCommits(ctx context.Context, commits []*gitlib.Commit, indexOffset, chunkIndex int) (PipelineStats, error) {
	runner.runtimeTuningOnce.Do(func() {
		runner.runtimeBallast = applyRuntimeTuning(runner.Config, runner.MemBudget)
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

		analyzeCtx := runner.buildAnalyzeContext(data, indexOffset)

		consumeErr := runner.consumeAll(ctx, analyzeCtx, analyzerDurations)
		if consumeErr != nil {
			observability.RecordSpanError(span, consumeErr, observability.ErrTypeInternal, observability.ErrSourceServer)
			span.End()

			return PipelineStats{}, consumeErr
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

	// TC stamping metadata, captured on the main goroutine after core analyzers run.
	tick      int
	authorID  int
	timestamp time.Time
}

// bufferedTC holds a TC and its original analyzer index for deferred aggregation.
type bufferedTC struct {
	tc   analyze.TC
	idx  int // original index in runner.Analyzers.
	time time.Time
}

// leafWorker holds forked leaf analyzers for one worker goroutine.
type leafWorker struct {
	leaves    []analyze.HistoryAnalyzer
	indices   []int // original indices in runner.Analyzers for each leaf.
	workChan  chan leafWork
	durations []time.Duration // Accumulated per-leaf-analyzer durations.
	tcs       []bufferedTC    // buffered TCs for deferred aggregation.
}

// processWork applies the plumbing snapshot, runs leaf Consume(), then releases snapshot resources.
// TCs with non-nil Data are buffered for deferred aggregation on the main goroutine.
func (w *leafWorker) processWork(ctx context.Context, work leafWork) error {
	for i, leaf := range w.leaves {
		p, ok := leaf.(analyze.Parallelizable)
		if !ok {
			return fmt.Errorf("%w: %s", ErrNotParallelizable, leaf.Name())
		}

		p.ApplySnapshot(work.snapshot)

		start := time.Now()

		tc, consumeErr := leaf.Consume(ctx, work.analyzeCtx)

		w.durations[i] += time.Since(start)

		if consumeErr != nil {
			return consumeErr
		}

		if tc.Data != nil {
			tc.Tick = work.tick
			tc.AuthorID = work.authorID
			tc.Timestamp = work.timestamp
			w.tcs = append(w.tcs, bufferedTC{tc: tc, idx: w.indices[i], time: work.timestamp})
		}
	}

	// Release snapshot resources (e.g. UAST trees).
	releaseSnapshot(work.snapshot)

	return nil
}

// newLeafWorkers creates W leaf workers with forked leaf analyzers.
// Each forked leaf owns independent plumbing struct copies (created by Fork()).
// leafIndices maps each leaf position to its original index in runner.Analyzers.
func newLeafWorkers(leaves []analyze.HistoryAnalyzer, leafIndices []int, w int) []*leafWorker {
	// leafWorkChanBuffer is the channel buffer size for each leaf worker.
	// A small buffer allows one commit to be queued while another is being processed.
	const leafWorkChanBuffer = 2

	workers := make([]*leafWorker, w)

	for i := range w {
		worker := &leafWorker{
			workChan:  make(chan leafWork, leafWorkChanBuffer),
			indices:   leafIndices,
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

	return composite.Clone()
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
func (runner *Runner) buildAnalyzeContext(data CommitData, indexOffset int) *analyze.Context {
	commit := data.Commit

	isMerge := commit.NumParents() > 1
	if runner.Config.FirstParent {
		isMerge = false
	}

	return &analyze.Context{
		Commit:      commit,
		Index:       data.Index + indexOffset,
		Time:        commit.Committer().When,
		IsMerge:     isMerge,
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
	idxMap := runner.analyzerIndex()

	numWorkers := runner.Config.LeafWorkers
	workers := newLeafWorkers(cpuHeavy, mapIndices(cpuHeavy, idxMap), numWorkers)
	wg, workerErrors := startLeafWorkers(ctx, workers)

	snapshotters, snapErr := collectSnapshotters(append(cpuHeavy, lightweight...))
	if snapErr != nil {
		span.End()

		return PipelineStats{}, snapErr
	}

	// Combine lightweight and serial leaves into one main-goroutine group.
	mainLeaves := make([]analyze.HistoryAnalyzer, 0, len(lightweight)+len(serialLeaves))
	mainLeaves = append(mainLeaves, lightweight...)
	mainLeaves = append(mainLeaves, serialLeaves...)
	mainIndices := mapIndices(mainLeaves, idxMap)

	coreDurations, mainDurations, loopErr := runner.hybridCommitLoop(
		ctx, dataChan, indexOffset, core, mainLeaves, mainIndices, snapshotters, workers, numWorkers, wg)
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

	// Drain buffered TCs from workers into aggregators on the main goroutine.
	runner.drainWorkerTCs(workers)

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
// mainIndices maps each serial leaf position to its original index in runner.Analyzers.
// Returns accumulated durations for core and main-goroutine leaf analyzers.
func (runner *Runner) hybridCommitLoop(
	ctx context.Context,
	dataChan <-chan CommitData,
	indexOffset int,
	core, serialLeaves []analyze.HistoryAnalyzer,
	mainIndices []int,
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

		analyzeCtx := runner.buildAnalyzeContext(data, indexOffset)

		// Run core (plumbing) analyzers sequentially.
		for i, a := range core {
			start := time.Now()

			_, coreErr := a.Consume(ctx, analyzeCtx)

			coreDurations[i] += time.Since(start)

			if coreErr != nil {
				closeWorkersAndWait(workers, wg)

				return nil, nil, coreErr
			}
		}

		// Snapshot plumbing state for parallel workers before serial leaves mutate anything.
		// Build a composite snapshot from ALL parallel leaves so every plumbing field
		// (Changes, BlobCache, FileDiffs, UAST, Tick, AuthorID, etc.) is captured.
		work := runner.buildLeafWork(analyzeCtx, snapshotters)

		// Dispatch parallel leaves to a worker.
		workers[commitIdx%numWorkers].workChan <- work

		// Run serial leaves on the main goroutine.
		for i, a := range serialLeaves {
			start := time.Now()

			tc, leafErr := a.Consume(ctx, analyzeCtx)

			mainDurations[i] += time.Since(start)

			if leafErr != nil {
				closeWorkersAndWait(workers, wg)

				return nil, nil, leafErr
			}

			runner.addTC(tc, mainIndices[i], analyzeCtx)
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
