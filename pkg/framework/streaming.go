package framework

import (
	"context"
	"fmt"
	"io"
	"log/slog"
	"time"

	"go.opentelemetry.io/otel"
	"go.opentelemetry.io/otel/attribute"
	"go.opentelemetry.io/otel/trace"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/checkpoint"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
	"github.com/Sumatoshi-tech/codefang/pkg/observability"
	"github.com/Sumatoshi-tech/codefang/pkg/streaming"
)

// StreamingConfig holds configuration for streaming pipeline execution.
type StreamingConfig struct {
	MemBudget     int64
	Checkpoint    CheckpointParams
	RepoPath      string
	AnalyzerNames []string

	// Logger is the structured logger for streaming operations.
	// When nil, a discard logger is used.
	Logger *slog.Logger

	// DebugTrace enables 100% trace sampling for debugging.
	DebugTrace bool

	// AnalysisMetrics records analysis-specific OTel metrics (commits, chunks,
	// cache stats). Nil-safe: when nil, no metrics are recorded.
	AnalysisMetrics *observability.AnalysisMetrics
}

// logger returns the configured logger, or a discard logger if nil.
func (c StreamingConfig) logger() *slog.Logger {
	if c.Logger != nil {
		return c.Logger
	}

	return slog.New(slog.NewTextHandler(io.Discard, nil))
}

// RunStreaming executes the pipeline in streaming chunks with optional checkpoint support.
// When the memory budget is sufficient and multiple chunks are needed, it
// enables double-buffered chunk pipelining to overlap pipeline execution
// with analyzer consumption.
func RunStreaming(
	ctx context.Context,
	runner *Runner,
	commits []*gitlib.Commit,
	analyzers []analyze.HistoryAnalyzer,
	config StreamingConfig,
) (map[analyze.HistoryAnalyzer]analyze.Report, error) {
	logger := config.logger()
	growthPerCommit := aggregateStateGrowth(analyzers, runner.CoreCount)
	pipelineOverhead := runner.Config.EstimatedOverhead()
	chunks, useDoubleBuffer := planChunksWithDoubleBuffer(len(commits), config.MemBudget, growthPerCommit, pipelineOverhead)
	hibernatables := collectHibernatables(analyzers)
	checkpointables := collectCheckpointables(analyzers)

	cpManager := initCheckpointManager(ctx, logger, config.Checkpoint, config.RepoPath, len(analyzers), len(checkpointables))

	logger.InfoContext(ctx, "streaming: planning chunks",
		"commits", len(commits), "chunks", len(chunks), "double_buffer", useDoubleBuffer)

	startChunk := resolveStartChunk(ctx, logger, cpManager, checkpointables, config)

	if startChunk == 0 {
		err := runner.Initialize()
		if err != nil {
			return nil, fmt.Errorf("initialization failed: %w", err)
		}
	}

	chunkSize := 0
	if len(chunks) > 0 {
		chunkSize = chunks[0].End - chunks[0].Start
	}

	tr := otel.Tracer(tracerName)
	ctx, analysisSpan := tr.Start(ctx, "codefang.analysis",
		trace.WithAttributes(
			attribute.Int("analysis.chunks", len(chunks)),
			attribute.Int("analysis.chunk_size", chunkSize),
			attribute.Bool("analysis.double_buffered", useDoubleBuffer),
		))

	var stats chunkStats

	var err error

	if useDoubleBuffer {
		stats, err = processChunksDoubleBuffered(
			ctx, logger, runner, commits, chunks, hibernatables, checkpointables,
			cpManager, config.RepoPath, config.AnalyzerNames, startChunk,
		)
	} else {
		stats, err = processChunksWithCheckpoint(
			ctx, logger, runner, commits, chunks, hibernatables, checkpointables,
			cpManager, config.RepoPath, config.AnalyzerNames, startChunk,
		)
	}

	setAnalysisSpanAttributes(analysisSpan, stats)
	analysisSpan.End()
	recordAnalysisMetrics(ctx, config.AnalysisMetrics, stats, len(commits))

	if err != nil {
		return nil, err
	}

	if cpManager != nil {
		clearErr := cpManager.Clear()
		if clearErr != nil {
			logger.WarnContext(ctx, "failed to clear checkpoint after completion", "error", clearErr)
		}
	}

	return runner.Finalize()
}

// chunkStats holds aggregate timing and pipeline metrics across all chunks.
type chunkStats struct {
	totalNS        int64
	count          int
	pipeline       PipelineStats
	chunkDurations []time.Duration

	// Slowest chunk details.
	slowestMS     int64
	slowestIndex  int
	slowestOffset int
	slowestSize   int
}

// record updates stats with a chunk's duration.
func (s *chunkStats) record(dur time.Duration, idx int, chunk streaming.ChunkBounds) {
	ms := dur.Milliseconds()

	s.count++
	s.totalNS += dur.Nanoseconds()
	s.chunkDurations = append(s.chunkDurations, dur)

	if ms > s.slowestMS {
		s.slowestMS = ms
		s.slowestIndex = idx
		s.slowestOffset = chunk.Start
		s.slowestSize = chunk.End - chunk.Start
	}
}

// setAnalysisSpanAttributes sets aggregate timing, pipeline, and cache attributes
// on the analysis span.
func setAnalysisSpanAttributes(span trace.Span, stats chunkStats) {
	ps := stats.pipeline

	span.SetAttributes(
		attribute.Int64("analysis.slowest_chunk_ms", stats.slowestMS),
		attribute.Int64("analysis.total_chunk_ms", stats.totalNS/int64(time.Millisecond)),
		// Pipeline aggregate timing.
		attribute.Int64("analysis.pipeline.blob_ms", ps.BlobDuration.Milliseconds()),
		attribute.Int64("analysis.pipeline.diff_ms", ps.DiffDuration.Milliseconds()),
		attribute.Int64("analysis.pipeline.uast_ms", ps.UASTDuration.Milliseconds()),
		attribute.String("analysis.pipeline.dominant", dominantStage(ps)),
		// Cache aggregate stats.
		attribute.Int64("analysis.cache.blob.hits", ps.BlobCacheHits),
		attribute.Int64("analysis.cache.blob.misses", ps.BlobCacheMisses),
		attribute.Float64("analysis.cache.blob.hit_pct", hitPercent(ps.BlobCacheHits, ps.BlobCacheMisses)),
		attribute.Int64("analysis.cache.diff.hits", ps.DiffCacheHits),
		attribute.Int64("analysis.cache.diff.misses", ps.DiffCacheMisses),
		attribute.Float64("analysis.cache.diff.hit_pct", hitPercent(ps.DiffCacheHits, ps.DiffCacheMisses)),
	)

	if stats.count > 0 {
		span.AddEvent("analysis.slowest_chunk", trace.WithAttributes(
			attribute.Int("chunk.index", stats.slowestIndex),
			attribute.Int("chunk.offset", stats.slowestOffset),
			attribute.Int("chunk.size", stats.slowestSize),
			attribute.Int64("chunk.duration_ms", stats.slowestMS),
		))
	}
}

// recordAnalysisMetrics records analysis-specific OTel metrics from chunk stats.
func recordAnalysisMetrics(ctx context.Context, am *observability.AnalysisMetrics, stats chunkStats, commitCount int) {
	am.RecordRun(ctx, observability.AnalysisStats{
		Commits:         int64(commitCount),
		Chunks:          stats.count,
		ChunkDurations:  stats.chunkDurations,
		BlobCacheHits:   stats.pipeline.BlobCacheHits,
		BlobCacheMisses: stats.pipeline.BlobCacheMisses,
		DiffCacheHits:   stats.pipeline.DiffCacheHits,
		DiffCacheMisses: stats.pipeline.DiffCacheMisses,
	})
}

// planChunksWithDoubleBuffer plans chunk boundaries, enabling double-buffering
// when memory budget allows. When double-buffering is active, the budget is
// halved to accommodate two concurrent chunks, which may result in more (smaller)
// chunks but the pipeline overlap compensates.
func planChunksWithDoubleBuffer(commitCount int, memBudget, growthPerCommit, pipelineOverhead int64) ([]streaming.ChunkBounds, bool) {
	// First pass: plan with full budget to determine chunk count.
	initialChunks := planChunks(commitCount, memBudget, growthPerCommit, pipelineOverhead)

	if !canDoubleBuffer(memBudget, len(initialChunks)) {
		return initialChunks, false
	}

	// Re-plan for double-buffering: two analyzer states share the memory left
	// after pipeline overhead. Double the overhead so each slot gets half the
	// remaining state budget, while caches/workers stay shared.
	dbOverhead := pipelineOverhead
	if dbOverhead <= 0 {
		dbOverhead = int64(streaming.BaseOverhead)
	}

	stateForTwo := memBudget - dbOverhead
	if stateForTwo <= 0 {
		return initialChunks, false
	}

	// Synthetic overhead that leaves each slot half the state budget.
	syntheticOverhead := dbOverhead + stateForTwo/doubleBufferBudgetDivisor
	dbChunks := planChunks(commitCount, memBudget, growthPerCommit, syntheticOverhead)

	return dbChunks, true
}

// initCheckpointManager creates and validates a checkpoint manager, returning nil if
// checkpointing is disabled or the analyzer set doesn't fully support it.
func initCheckpointManager(
	ctx context.Context, logger *slog.Logger, cpConfig CheckpointParams, repoPath string,
	totalAnalyzers, checkpointableCount int,
) *checkpoint.Manager {
	if !cpConfig.Enabled {
		return nil
	}

	repoHash := checkpoint.RepoHash(repoPath)
	cpManager := checkpoint.NewManager(cpConfig.Dir, repoHash)

	if cpConfig.ClearPrev {
		clearErr := cpManager.Clear()
		if clearErr != nil {
			logger.WarnContext(ctx, "failed to clear checkpoint", "error", clearErr)
		}
	}

	if !CanResumeWithCheckpoint(totalAnalyzers, checkpointableCount) {
		logger.WarnContext(ctx, "checkpoint: disabled; incomplete support",
			"checkpointable", checkpointableCount,
			"total", totalAnalyzers,
		)

		return nil
	}

	return cpManager
}

// resolveStartChunk determines which chunk to start from, attempting checkpoint
// resume if configured and available.
func resolveStartChunk(
	ctx context.Context, logger *slog.Logger, cpManager *checkpoint.Manager,
	checkpointables []checkpoint.Checkpointable, config StreamingConfig,
) int {
	if cpManager == nil || !config.Checkpoint.Resume || !cpManager.Exists() {
		return 0
	}

	resumedChunk, err := tryResumeFromCheckpoint(cpManager, checkpointables, config.RepoPath, config.AnalyzerNames)
	if err != nil {
		logger.WarnContext(ctx, "checkpoint: resume failed, starting fresh", "error", err)

		return 0
	}

	if resumedChunk > 0 {
		logger.InfoContext(ctx, "checkpoint: resuming", "chunk", resumedChunk+1)

		trace.SpanFromContext(ctx).AddEvent("checkpoint.resumed", trace.WithAttributes(
			attribute.Int("chunk", resumedChunk+1),
		))
	}

	return resumedChunk
}

// CanResumeWithCheckpoint returns true if all analyzers support checkpointing.
func CanResumeWithCheckpoint(totalAnalyzers, checkpointableCount int) bool {
	if totalAnalyzers <= 0 {
		return false
	}

	return checkpointableCount == totalAnalyzers
}

func planChunks(commitCount int, memBudget, growthPerCommit, pipelineOverhead int64) []streaming.ChunkBounds {
	planner := streaming.Planner{
		TotalCommits:             commitCount,
		MemoryBudget:             memBudget,
		AggregateGrowthPerCommit: growthPerCommit,
		PipelineOverhead:         pipelineOverhead,
	}

	return planner.Plan()
}

// aggregateStateGrowth sums the per-commit state growth of selected leaf analyzers.
// Core/plumbing analyzers (indices < coreCount) are skipped. Analyzers that don't
// implement MemoryWeighter use DefaultStateGrowthPerCommit.
func aggregateStateGrowth(analyzers []analyze.HistoryAnalyzer, coreCount int) int64 {
	var total int64

	for i, a := range analyzers {
		if i < coreCount {
			continue
		}

		total += streaming.GrowthOrDefault(a)
	}

	if total <= 0 {
		return streaming.DefaultStateGrowthPerCommit
	}

	return total
}

func collectHibernatables(analyzers []analyze.HistoryAnalyzer) []streaming.Hibernatable {
	var hibernatables []streaming.Hibernatable

	for _, a := range analyzers {
		if h, ok := a.(streaming.Hibernatable); ok {
			hibernatables = append(hibernatables, h)
		}
	}

	return hibernatables
}

func collectCheckpointables(analyzers []analyze.HistoryAnalyzer) []checkpoint.Checkpointable {
	var checkpointables []checkpoint.Checkpointable

	for _, a := range analyzers {
		if c, ok := a.(checkpoint.Checkpointable); ok {
			checkpointables = append(checkpointables, c)
		}
	}

	return checkpointables
}

func tryResumeFromCheckpoint(
	cpManager *checkpoint.Manager,
	checkpointables []checkpoint.Checkpointable,
	repoPath string,
	analyzerNames []string,
) (int, error) {
	validateErr := cpManager.Validate(repoPath, analyzerNames)
	if validateErr != nil {
		return 0, fmt.Errorf("checkpoint validation failed: %w", validateErr)
	}

	state, err := cpManager.Load(checkpointables)
	if err != nil {
		return 0, fmt.Errorf("checkpoint load failed: %w", err)
	}

	return state.CurrentChunk + 1, nil
}

func processChunksWithCheckpoint(
	ctx context.Context,
	logger *slog.Logger,
	runner *Runner,
	commits []*gitlib.Commit,
	chunks []streaming.ChunkBounds,
	hibernatables []streaming.Hibernatable,
	checkpointables []checkpoint.Checkpointable,
	cpManager *checkpoint.Manager,
	repoPath string,
	analyzerNames []string,
	startChunk int,
) (chunkStats, error) {
	var stats chunkStats

	for i := startChunk; i < len(chunks); i++ {
		chunk := chunks[i]
		logger.InfoContext(ctx, "streaming: processing chunk",
			"chunk", i+1, "total", len(chunks), "start", chunk.Start, "end", chunk.End)

		if i > startChunk {
			hibErr := hibernateAndBoot(hibernatables)
			if hibErr != nil {
				return stats, hibErr
			}
		}

		chunkCommits := commits[chunk.Start:chunk.End]

		start := time.Now()

		pStats, err := runner.ProcessChunk(ctx, chunkCommits, chunk.Start, i)
		if err != nil {
			return stats, fmt.Errorf("chunk %d failed: %w", i+1, err)
		}

		stats.record(time.Since(start), i, chunk)
		stats.pipeline.Add(pStats)

		saveChunkCheckpoint(ctx, logger, cpManager, checkpointables, commits, chunk, chunks, i, repoPath, analyzerNames)
	}

	return stats, nil
}

// doubleBufferBudgetDivisor is the factor by which available memory is divided
// when double-buffering is active (two chunks in flight simultaneously).
const doubleBufferBudgetDivisor = 2

// minDoubleBufferAvailable is the minimum available memory (after overhead)
// required to enable double-buffering. Each slot needs enough room for at
// least MinChunkSize commits of state growth.
const minDoubleBufferAvailable = int64(doubleBufferBudgetDivisor * streaming.MinChunkSize * streaming.DefaultStateGrowthPerCommit)

// minDoubleBufferChunks is the minimum number of chunks required for
// double-buffering to provide any benefit.
const minDoubleBufferChunks = 2

// doubleBufferMemoryBudget computes the per-chunk memory budget when
// double-buffering is active. It halves the available budget (total minus
// fixed overhead), then adds overhead back so each chunk's planner sees a
// realistic budget. Returns the original budget unchanged if it is zero or
// too small to split.
func doubleBufferMemoryBudget(totalBudget int64) int64 {
	if totalBudget <= 0 {
		return totalBudget
	}

	available := totalBudget - int64(streaming.BaseOverhead)
	if available <= 0 {
		return totalBudget
	}

	return available/doubleBufferBudgetDivisor + int64(streaming.BaseOverhead)
}

// prefetchedChunk holds the pre-fetched pipeline output for one chunk.
// The data slice preserves commit ordering from the Coordinator.
type prefetchedChunk struct {
	data  []CommitData
	stats PipelineStats
	err   error
}

// prefetchPipeline opens a fresh repo handle, runs the Coordinator pipeline
// for the given commits, and collects all CommitData into a prefetchedChunk.
// The caller does not need to close the repo; it is freed internally.
func prefetchPipeline(
	ctx context.Context, repoPath string, config CoordinatorConfig,
	commits []*gitlib.Commit, _ trace.Tracer,
) prefetchedChunk {
	repo, openErr := gitlib.OpenRepository(repoPath)
	if openErr != nil {
		return prefetchedChunk{err: fmt.Errorf("prefetch: open repository: %w", openErr)}
	}

	coordinator := NewCoordinator(repo, config)
	dataChan := coordinator.Process(ctx, commits)

	var collected []CommitData

	for cd := range dataChan {
		if cd.Error != nil {
			repo.Free()

			return prefetchedChunk{err: cd.Error}
		}

		collected = append(collected, cd)
	}

	repo.Free()

	return prefetchedChunk{data: collected, stats: coordinator.Stats()}
}

// startPrefetch launches prefetchPipeline in a background goroutine and
// returns a channel that delivers the result exactly once.
func startPrefetch(
	ctx context.Context, repoPath string, config CoordinatorConfig,
	commits []*gitlib.Commit, tracer trace.Tracer,
) <-chan prefetchedChunk {
	ch := make(chan prefetchedChunk, 1)

	go func() {
		ch <- prefetchPipeline(ctx, repoPath, config, commits, tracer)

		close(ch)
	}()

	return ch
}

// canDoubleBuffer returns true when the memory budget and chunk count are
// sufficient to benefit from double-buffered chunk pipelining.
func canDoubleBuffer(memBudget int64, chunkCount int) bool {
	if chunkCount < minDoubleBufferChunks {
		return false
	}

	if memBudget <= 0 {
		return false
	}

	available := memBudget - int64(streaming.BaseOverhead)

	return available >= minDoubleBufferAvailable
}

// doubleBufferState holds parameters shared across the double-buffered chunk loop.
type doubleBufferState struct {
	runner          *Runner
	commits         []*gitlib.Commit
	chunks          []streaming.ChunkBounds
	hibernatables   []streaming.Hibernatable
	checkpointables []checkpoint.Checkpointable
	cpManager       *checkpoint.Manager
	repoPath        string
	analyzerNames   []string
	logger          *slog.Logger
}

// processChunksDoubleBuffered overlaps chunk K+1's pipeline with chunk K's
// analyzer consumption. The first chunk runs normally (no prefetch available).
// For each subsequent chunk, the pipeline was started during the previous
// chunk's consumption, so data is immediately available.
func processChunksDoubleBuffered(
	ctx context.Context,
	logger *slog.Logger,
	runner *Runner,
	commits []*gitlib.Commit,
	chunks []streaming.ChunkBounds,
	hibernatables []streaming.Hibernatable,
	checkpointables []checkpoint.Checkpointable,
	cpManager *checkpoint.Manager,
	repoPath string,
	analyzerNames []string,
	startChunk int,
) (chunkStats, error) {
	var stats chunkStats

	st := &doubleBufferState{
		runner:          runner,
		commits:         commits,
		chunks:          chunks,
		hibernatables:   hibernatables,
		checkpointables: checkpointables,
		cpManager:       cpManager,
		repoPath:        repoPath,
		analyzerNames:   analyzerNames,
		logger:          logger,
	}

	for idx := startChunk; idx < len(chunks); idx++ {
		prefetch := st.startNextPrefetch(ctx, idx)

		dur, pStats, err := st.processCurrentChunk(ctx, idx, startChunk)
		if err != nil {
			drainPrefetch(prefetch)

			return stats, err
		}

		stats.record(dur, idx, st.chunks[idx])
		stats.pipeline.Add(pStats)

		consumed, consumeDur, consumePStats, consumeErr := st.consumePrefetched(ctx, idx, prefetch)
		if consumeErr != nil {
			return stats, consumeErr
		}

		if consumed {
			stats.record(consumeDur, idx+1, st.chunks[idx+1])
			stats.pipeline.Add(consumePStats)

			idx++ // Skip the prefetched chunk in the loop.
		}
	}

	return stats, nil
}

// startNextPrefetch begins pipeline execution for chunk idx+1 in a goroutine.
// Returns nil if there is no next chunk.
func (st *doubleBufferState) startNextPrefetch(ctx context.Context, idx int) <-chan prefetchedChunk {
	nextIdx := idx + 1
	if nextIdx >= len(st.chunks) {
		return nil
	}

	nextChunk := st.chunks[nextIdx]
	nextCommits := st.commits[nextChunk.Start:nextChunk.End]

	return startPrefetch(ctx, st.repoPath, st.runner.Config, nextCommits, st.runner.tracer())
}

// processCurrentChunk hibernates (if not the first chunk), runs the pipeline
// through the Coordinator, and saves a checkpoint. Returns the chunk processing
// duration and pipeline stats.
func (st *doubleBufferState) processCurrentChunk(ctx context.Context, idx, startChunk int) (time.Duration, PipelineStats, error) {
	chunk := st.chunks[idx]
	chunkCommits := st.commits[chunk.Start:chunk.End]

	st.logger.InfoContext(ctx, "streaming[db]: processing chunk",
		"chunk", idx+1, "total", len(st.chunks), "start", chunk.Start, "end", chunk.End)

	if idx > startChunk {
		hibErr := hibernateAndBoot(st.hibernatables)
		if hibErr != nil {
			return 0, PipelineStats{}, hibErr
		}
	}

	start := time.Now()

	pStats, processErr := st.runner.ProcessChunk(ctx, chunkCommits, chunk.Start, idx)
	if processErr != nil {
		return 0, PipelineStats{}, fmt.Errorf("chunk %d failed: %w", idx+1, processErr)
	}

	dur := time.Since(start)

	saveChunkCheckpoint(ctx, st.logger, st.cpManager, st.checkpointables, st.commits, chunk, st.chunks, idx, st.repoPath, st.analyzerNames)

	return dur, pStats, nil
}

// consumePrefetched waits for the prefetched result, hibernates, and feeds it
// to analyzers. Returns (consumed, duration, pipeline stats, error).
func (st *doubleBufferState) consumePrefetched(
	ctx context.Context, idx int, prefetch <-chan prefetchedChunk,
) (bool, time.Duration, PipelineStats, error) {
	if prefetch == nil {
		return false, 0, PipelineStats{}, nil
	}

	pf := <-prefetch

	nextIdx := idx + 1
	nextChunk := st.chunks[nextIdx]

	if pf.err != nil {
		return false, 0, PipelineStats{}, fmt.Errorf("prefetch chunk %d failed: %w", nextIdx+1, pf.err)
	}

	st.logger.InfoContext(ctx, "streaming[db]: consuming prefetched chunk",
		"chunk", nextIdx+1, "total", len(st.chunks), "start", nextChunk.Start, "end", nextChunk.End)

	hibErr := hibernateAndBoot(st.hibernatables)
	if hibErr != nil {
		return false, 0, PipelineStats{}, hibErr
	}

	start := time.Now()

	_, processErr := st.runner.ProcessChunkFromData(ctx, pf.data, nextChunk.Start, nextIdx)
	if processErr != nil {
		return false, 0, PipelineStats{}, fmt.Errorf("chunk %d failed: %w", nextIdx+1, processErr)
	}

	dur := time.Since(start)

	saveChunkCheckpoint(
		ctx, st.logger, st.cpManager, st.checkpointables, st.commits,
		nextChunk, st.chunks, nextIdx, st.repoPath, st.analyzerNames,
	)

	// Use prefetch pipeline stats since ProcessChunkFromData returns zero stats.
	return true, dur, pf.stats, nil
}

// drainPrefetchTimeout is the maximum time to wait for a pending prefetch to
// complete before abandoning it. This prevents indefinite blocking when a
// prefetch goroutine hangs (e.g. due to a stalled CGO call).
const drainPrefetchTimeout = 30 * time.Second

// drainPrefetch waits for a pending prefetch to complete (if any) to prevent
// goroutine leaks. The result is discarded. If the prefetch does not complete
// within drainPrefetchTimeout, it is abandoned.
func drainPrefetch(ch <-chan prefetchedChunk) {
	if ch == nil {
		return
	}

	select {
	case <-ch:
	case <-time.After(drainPrefetchTimeout):
	}
}

// saveChunkCheckpoint saves a checkpoint after processing a chunk (if checkpointing is enabled
// and this is not the last chunk).
func saveChunkCheckpoint(
	ctx context.Context,
	logger *slog.Logger,
	cpManager *checkpoint.Manager,
	checkpointables []checkpoint.Checkpointable,
	commits []*gitlib.Commit,
	chunk streaming.ChunkBounds,
	chunks []streaming.ChunkBounds,
	chunkIdx int,
	repoPath string,
	analyzerNames []string,
) {
	if cpManager == nil || chunkIdx >= len(chunks)-1 {
		return
	}

	chunkCommits := commits[chunk.Start:chunk.End]
	lastCommit := chunkCommits[len(chunkCommits)-1]

	state := checkpoint.StreamingState{
		TotalCommits:     len(commits),
		ProcessedCommits: chunk.End,
		CurrentChunk:     chunkIdx,
		TotalChunks:      len(chunks),
		LastCommitHash:   lastCommit.Hash().String(),
	}

	saveErr := cpManager.Save(checkpointables, state, repoPath, analyzerNames)
	if saveErr != nil {
		logger.WarnContext(ctx, "failed to save checkpoint", "error", saveErr)
	} else {
		logger.InfoContext(ctx, "checkpoint: saved", "chunk", chunkIdx+1)

		trace.SpanFromContext(ctx).AddEvent("checkpoint.saved", trace.WithAttributes(
			attribute.Int("chunk", chunkIdx+1),
		))
	}
}

// dominantStage returns the name of the pipeline stage with the longest duration.
func dominantStage(ps PipelineStats) string {
	switch {
	case ps.UASTDuration >= ps.BlobDuration && ps.UASTDuration >= ps.DiffDuration:
		return "uast"
	case ps.BlobDuration >= ps.DiffDuration:
		return "blob"
	default:
		return "diff"
	}
}

// hitPercent computes the cache hit percentage. Returns 0 when there are no lookups.
func hitPercent(hits, misses int64) float64 {
	total := hits + misses
	if total == 0 {
		return 0
	}

	const percentScale = 100.0

	return float64(hits) / float64(total) * percentScale
}

func hibernateAndBoot(hibernatables []streaming.Hibernatable) error {
	for _, h := range hibernatables {
		err := h.Hibernate()
		if err != nil {
			return fmt.Errorf("hibernation failed: %w", err)
		}
	}

	for _, h := range hibernatables {
		err := h.Boot()
		if err != nil {
			return fmt.Errorf("boot failed: %w", err)
		}
	}

	return nil
}
