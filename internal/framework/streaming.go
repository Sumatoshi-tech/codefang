package framework

import (
	"context"
	"errors"
	"fmt"
	"io"
	"log/slog"
	"runtime"
	"runtime/debug"
	"strings"
	"time"

	"go.opentelemetry.io/otel"
	"go.opentelemetry.io/otel/attribute"
	"go.opentelemetry.io/otel/trace"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/internal/checkpoint"
	"github.com/Sumatoshi-tech/codefang/internal/observability"
	"github.com/Sumatoshi-tech/codefang/internal/streaming"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

// errMissingInterfaces is returned when leaf analyzers lack required streaming interfaces.
var errMissingInterfaces = errors.New("streaming: leaf analyzers missing required interfaces")

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

	// TCSink, when set, receives every non-nil TC as commits are consumed.
	// Used by NDJSON streaming output. When set, aggregators are not created
	// and FinalizeWithAggregators is not called — results are nil.
	TCSink analyze.TCSink

	// AggSpillBudget is the maximum bytes of aggregator state to keep in memory
	// before spilling to disk. Computed by ComputeSchedule. Zero means no limit.
	AggSpillBudget int64

	// OnChunkComplete, when set, is called after each chunk finishes processing.
	// Used by streaming timeseries NDJSON to drain per-commit data from aggregators
	// between chunks, keeping memory bounded by chunk size.
	OnChunkComplete func(runner *Runner) error

	// SkipFinalize, when true, causes RunStreaming/RunStreamingFromIterator to
	// return empty reports instead of calling FinalizeWithAggregators. Used when
	// OnChunkComplete already handles output (e.g. streaming timeseries NDJSON).
	SkipFinalize bool
}

// logger returns the configured logger, or a discard logger if nil.
func (c StreamingConfig) logger() *slog.Logger {
	if c.Logger != nil {
		return c.Logger
	}

	return slog.New(slog.NewTextHandler(io.Discard, nil))
}

// maxStreamingBuffering is the maximum buffering factor for RunStreaming.
// Triple-buffering prefetches 2 chunks ahead for maximum pipeline overlap.
const maxStreamingBuffering = 3

// RunStreaming executes the pipeline in streaming chunks with optional checkpoint support.
// The scheduler determines the buffering factor (single/double/triple) based on the
// memory budget. Higher buffering factors overlap more pipeline phases with analyzer
// consumption but require smaller chunks.
//
// When the commit set is trivially small (single commit) and no streaming features are
// needed, delegates to Runner.Run for simplicity and lower overhead.
func RunStreaming( //nolint:funlen // sequential pipeline setup.
	ctx context.Context,
	runner *Runner,
	commits []*gitlib.Commit,
	analyzers []analyze.HistoryAnalyzer,
	config StreamingConfig,
) (map[analyze.HistoryAnalyzer]analyze.Report, error) {
	// Short-circuit: for a single commit with no TCSink or checkpoint,
	// Runner.Run is simpler and avoids streaming overhead.
	if len(commits) == 1 && config.TCSink == nil && config.Checkpoint.Dir == "" {
		return runner.Run(ctx, commits)
	}

	logger := config.logger()
	growthPerCommit := aggregateStateGrowth(analyzers, runner.CoreCount)
	pipelineOverhead := runner.Config.EstimatedOverhead()
	workStatePerCommit, avgTCSize := splitStateGrowth(analyzers, runner.CoreCount)

	// Compute budget decomposition: chunks, buffering factor, and aggregator spill budget.
	schedule := streaming.ComputeSchedule(streaming.SchedulerConfig{
		TotalCommits:       len(commits),
		MemoryBudget:       config.MemBudget,
		PipelineOverhead:   pipelineOverhead,
		WorkStatePerCommit: workStatePerCommit,
		AvgTCSize:          avgTCSize,
		MaxBuffering:       maxStreamingBuffering,
	})

	chunks := schedule.Chunks

	// Create adaptive planner for feedback-driven replanning of remaining chunks.
	// Seed with per-slot growth so replanning produces chunks consistent with
	// the scheduler's buffering factor.
	perSlotGrowth := growthPerCommit / int64(max(schedule.BufferingFactor, 1))
	if perSlotGrowth <= 0 {
		perSlotGrowth = streaming.DefaultStateGrowthPerCommit
	}

	ap := streaming.NewAdaptivePlanner(len(commits), config.MemBudget, perSlotGrowth, pipelineOverhead)

	// Align debug.SetMemoryLimit with the user's budget.
	runner.MemBudget = config.MemBudget
	runner.TCSink = config.TCSink
	runner.AggSpillBudget = schedule.AggSpillBudget

	err := validateStreamingInterfaces(analyzers, runner.CoreCount)
	if err != nil {
		return nil, err
	}

	hibernatables := collectHibernatables(analyzers)
	spillCleaners := collectSpillCleaners(analyzers)
	checkpointables := collectCheckpointables(analyzers)

	// Guard ensures spill temp files are cleaned up on normal exit, error, or signal.
	spillGuard := streaming.NewSpillCleanupGuard(spillCleaners, logger)
	defer spillGuard.Close()

	cpManager := initCheckpointManager(ctx, logger, config.Checkpoint, config.RepoPath, len(analyzers), len(checkpointables))

	useDoubleBuffer := schedule.BufferingFactor >= doubleBufferBudgetDivisor

	logger.InfoContext(ctx, "streaming: planning chunks",
		"commits", len(commits), "chunks", len(chunks),
		"buffering_factor", schedule.BufferingFactor,
		"chunk_size", schedule.ChunkSize)

	startChunk, aggSpills := resolveStartChunk(ctx, logger, cpManager, checkpointables, chunks, config)

	initErr := initOrResume(runner, startChunk, aggSpills)
	if initErr != nil {
		return nil, initErr
	}

	_, err = runChunks(ctx, logger, runner, commits, chunks, useDoubleBuffer,
		hibernatables, checkpointables, cpManager, config, startChunk, ap)
	if err != nil {
		return nil, err
	}

	if cpManager != nil {
		clearErr := cpManager.Clear()
		if clearErr != nil {
			logger.WarnContext(ctx, "failed to clear checkpoint after completion", "error", clearErr)
		}
	}

	// In TCSink mode (NDJSON) or SkipFinalize mode (streaming timeseries NDJSON),
	// output was already written. Return empty (non-nil) map.
	if config.TCSink != nil || config.SkipFinalize {
		return make(map[analyze.HistoryAnalyzer]analyze.Report), nil
	}

	return runner.FinalizeWithAggregators(ctx)
}

// RunStreamingFromIterator executes the pipeline using a commit iterator instead
// of a pre-loaded commit slice. Commits are loaded chunk-at-a-time from the
// iterator and freed after processing, keeping memory usage proportional to
// chunk size rather than total repository size. Double-buffering is not supported
// in iterator mode; use RunStreaming with a pre-loaded slice for that.
func RunStreamingFromIterator( //nolint:funlen // sequential pipeline setup.
	ctx context.Context,
	runner *Runner,
	iter *gitlib.CommitIter,
	commitCount int,
	analyzers []analyze.HistoryAnalyzer,
	config StreamingConfig,
) (map[analyze.HistoryAnalyzer]analyze.Report, error) {
	logger := config.logger()
	pipelineOverhead := runner.Config.EstimatedOverhead()
	workStatePerCommit, avgTCSize := splitStateGrowth(analyzers, runner.CoreCount)

	// Iterator mode: single-buffering only (cannot prefetch without random access).
	schedule := streaming.ComputeSchedule(streaming.SchedulerConfig{
		TotalCommits:       commitCount,
		MemoryBudget:       config.MemBudget,
		PipelineOverhead:   pipelineOverhead,
		WorkStatePerCommit: workStatePerCommit,
		AvgTCSize:          avgTCSize,
		MaxBuffering:       1,
	})

	growthPerCommit := aggregateStateGrowth(analyzers, runner.CoreCount)
	ap := streaming.NewAdaptivePlanner(commitCount, config.MemBudget, growthPerCommit, pipelineOverhead)
	chunks := schedule.Chunks

	runner.MemBudget = config.MemBudget
	runner.TCSink = config.TCSink
	runner.AggSpillBudget = schedule.AggSpillBudget

	err := validateStreamingInterfaces(analyzers, runner.CoreCount)
	if err != nil {
		return nil, err
	}

	hibernatables := collectHibernatables(analyzers)
	spillCleaners := collectSpillCleaners(analyzers)
	checkpointables := collectCheckpointables(analyzers)

	spillGuard := streaming.NewSpillCleanupGuard(spillCleaners, logger)
	defer spillGuard.Close()

	cpManager := initCheckpointManager(ctx, logger, config.Checkpoint, config.RepoPath, len(analyzers), len(checkpointables))

	logger.InfoContext(ctx, "streaming: planning chunks (iterator mode)",
		"commits", commitCount, "chunks", len(chunks))

	startChunk, aggSpills := resolveStartChunk(ctx, logger, cpManager, checkpointables, chunks, config)

	// Skip already-processed commits in the iterator.
	if startChunk > 0 && startChunk < len(chunks) {
		skipCount := chunks[startChunk].Start

		skipErr := iter.Skip(skipCount)
		if skipErr != nil {
			logger.WarnContext(ctx, "iterator skip failed, starting fresh", "error", skipErr)

			startChunk = 0
			aggSpills = nil
		}
	}

	initErr := initOrResume(runner, startChunk, aggSpills)
	if initErr != nil {
		return nil, initErr
	}

	_, err = runChunksFromIterator(ctx, logger, runner, iter, commitCount,
		chunks, hibernatables, checkpointables, cpManager, config, startChunk, ap)
	if err != nil {
		return nil, err
	}

	if cpManager != nil {
		clearErr := cpManager.Clear()
		if clearErr != nil {
			logger.WarnContext(ctx, "failed to clear checkpoint after completion", "error", clearErr)
		}
	}

	// In TCSink mode (NDJSON) or SkipFinalize mode (streaming timeseries NDJSON),
	// output was already written. Return empty (non-nil) map.
	if config.TCSink != nil || config.SkipFinalize {
		return make(map[analyze.HistoryAnalyzer]analyze.Report), nil
	}

	return runner.FinalizeWithAggregators(ctx)
}

// runChunksFromIterator creates an analysis span and runs single-buffered
// iterator-based chunk processing.
func runChunksFromIterator(
	ctx context.Context, logger *slog.Logger,
	runner *Runner, iter *gitlib.CommitIter, commitCount int,
	chunks []streaming.ChunkBounds,
	hibernatables []streaming.Hibernatable,
	checkpointables []checkpoint.Checkpointable,
	cpManager *checkpoint.Manager,
	config StreamingConfig, startChunk int,
	ap *streaming.AdaptivePlanner,
) (chunkStats, error) {
	chunkSize := 0
	if len(chunks) > 0 {
		chunkSize = chunks[0].End - chunks[0].Start
	}

	tr := otel.Tracer(tracerName)
	ctx, analysisSpan := tr.Start(ctx, "codefang.analysis",
		trace.WithAttributes(
			attribute.Int("analysis.chunks", len(chunks)),
			attribute.Int("analysis.chunk_size", chunkSize),
			attribute.Bool("analysis.double_buffered", false),
			attribute.Bool("analysis.iterator_mode", true),
		))

	stats, err := processChunksFromIterator(
		ctx, logger, runner, iter, commitCount, chunks, hibernatables, checkpointables,
		cpManager, config.RepoPath, config.AnalyzerNames, startChunk,
		ap, config.MemBudget, config.OnChunkComplete,
	)

	setAnalysisSpanAttributes(analysisSpan, stats)
	analysisSpan.End()
	recordAnalysisMetrics(ctx, config.AnalysisMetrics, stats, commitCount)

	return stats, err
}

// runChunks creates an analysis span and dispatches to single- or double-buffered
// chunk processing. Returns aggregate stats and any processing error.
func runChunks(
	ctx context.Context, logger *slog.Logger,
	runner *Runner, commits []*gitlib.Commit,
	chunks []streaming.ChunkBounds, useDoubleBuffer bool,
	hibernatables []streaming.Hibernatable,
	checkpointables []checkpoint.Checkpointable,
	cpManager *checkpoint.Manager,
	config StreamingConfig, startChunk int,
	ap *streaming.AdaptivePlanner,
) (chunkStats, error) {
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
			ap, config.MemBudget, config.OnChunkComplete,
		)
	} else {
		stats, err = processChunksWithCheckpoint(
			ctx, logger, runner, commits, chunks, hibernatables, checkpointables,
			cpManager, config.RepoPath, config.AnalyzerNames, startChunk,
			ap, config.MemBudget, config.OnChunkComplete,
		)
	}

	setAnalysisSpanAttributes(analysisSpan, stats)
	analysisSpan.End()
	recordAnalysisMetrics(ctx, config.AnalysisMetrics, stats, len(commits))

	return stats, err
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
// resume if configured and available. The chunks parameter is used to validate
// that checkpoint boundaries align with the current plan (which may differ from
// the original plan if adaptive replanning occurred before a crash).
func resolveStartChunk(
	ctx context.Context, logger *slog.Logger, cpManager *checkpoint.Manager,
	checkpointables []checkpoint.Checkpointable, chunks []streaming.ChunkBounds, config StreamingConfig,
) (int, []checkpoint.AggregatorSpillEntry) {
	if cpManager == nil || !config.Checkpoint.Resume || !cpManager.Exists() {
		return 0, nil
	}

	resumedChunk, processedCommits, aggSpills, err := tryResumeFromCheckpoint(
		cpManager, checkpointables, config.RepoPath, config.AnalyzerNames)
	if err != nil {
		logger.WarnContext(ctx, "checkpoint: resume failed, starting fresh", "error", err)

		return 0, nil
	}

	// Validate that chunk boundaries align with the checkpoint.
	if resumedChunk > 0 && resumedChunk < len(chunks) {
		expectedStart := chunks[resumedChunk].Start
		if expectedStart != processedCommits {
			logger.WarnContext(ctx, "checkpoint: chunk boundary mismatch after adaptive replan, restarting",
				"expected_start", expectedStart, "checkpoint_processed", processedCommits)

			return 0, nil
		}
	}

	if resumedChunk > 0 {
		logger.InfoContext(ctx, "checkpoint: resuming", "chunk", resumedChunk+1)

		trace.SpanFromContext(ctx).AddEvent("checkpoint.resumed", trace.WithAttributes(
			attribute.Int("chunk", resumedChunk+1),
		))
	}

	return resumedChunk, aggSpills
}

// initOrResume initializes the runner for a fresh run or resumes from a checkpoint.
func initOrResume(runner *Runner, startChunk int, aggSpills []checkpoint.AggregatorSpillEntry) error {
	if startChunk == 0 {
		return runner.Initialize()
	}

	return runner.InitializeForResume(aggSpills)
}

// CanResumeWithCheckpoint returns true if all analyzers support checkpointing.
func CanResumeWithCheckpoint(totalAnalyzers, checkpointableCount int) bool {
	if totalAnalyzers <= 0 {
		return false
	}

	return checkpointableCount == totalAnalyzers
}

// aggregateStateGrowth sums the per-commit state growth of selected leaf analyzers.
// Core/plumbing analyzers (indices < coreCount) are skipped. Each leaf analyzer
// contributes WorkingStateSize() + AvgTCSize() to the total.
func aggregateStateGrowth(analyzers []analyze.HistoryAnalyzer, coreCount int) int64 {
	var total int64

	for i, a := range analyzers {
		if i < coreCount {
			continue
		}

		total += a.WorkingStateSize() + a.AvgTCSize()
	}

	if total <= 0 {
		return streaming.DefaultStateGrowthPerCommit
	}

	return total
}

// splitStateGrowth returns separate per-commit working state and TC size totals.
func splitStateGrowth(analyzers []analyze.HistoryAnalyzer, coreCount int) (workState, tcSize int64) {
	for i, a := range analyzers {
		if i < coreCount {
			continue
		}

		workState += a.WorkingStateSize()
		tcSize += a.AvgTCSize()
	}

	if workState <= 0 {
		workState = streaming.DefaultWorkingStateSize
	}

	if tcSize <= 0 {
		tcSize = streaming.DefaultAvgTCSize
	}

	return workState, tcSize
}

// validateStreamingInterfaces checks that all leaf analyzers implement the
// required streaming interfaces. Returns an error listing non-compliant analyzers.
func validateStreamingInterfaces(analyzers []analyze.HistoryAnalyzer, coreCount int) error {
	var missing []string

	for i, a := range analyzers {
		if i < coreCount {
			continue // Skip plumbing analyzers.
		}

		if _, ok := a.(streaming.Hibernatable); !ok {
			missing = append(missing, a.Name()+": Hibernatable")
		}
	}

	if len(missing) > 0 {
		return fmt.Errorf("%w: %s", errMissingInterfaces, strings.Join(missing, "; "))
	}

	return nil
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

func collectSpillCleaners(analyzers []analyze.HistoryAnalyzer) []streaming.SpillCleaner {
	var cleaners []streaming.SpillCleaner

	for _, a := range analyzers {
		if sc, ok := a.(streaming.SpillCleaner); ok {
			cleaners = append(cleaners, sc)
		}
	}

	return cleaners
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
) (startChunk, processedCommits int, aggSpills []checkpoint.AggregatorSpillEntry, err error) {
	validateErr := cpManager.Validate(repoPath, analyzerNames)
	if validateErr != nil {
		return 0, 0, nil, fmt.Errorf("checkpoint validation failed: %w", validateErr)
	}

	state, loadErr := cpManager.Load(checkpointables)
	if loadErr != nil {
		return 0, 0, nil, fmt.Errorf("checkpoint load failed: %w", loadErr)
	}

	return state.CurrentChunk + 1, state.ProcessedCommits, state.AggregatorSpills, nil
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
	ap *streaming.AdaptivePlanner,
	memBudget int64,
	onChunkComplete func(runner *Runner) error,
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

		aggSizeBefore := runner.AggregatorStateSize()
		runner.ResetTCCount()

		before := streaming.TakeHeapSnapshot()

		chunkCommits := commits[chunk.Start:chunk.End]

		start := time.Now()

		pStats, err := runner.ProcessChunk(ctx, chunkCommits, chunk.Start, i)
		if err != nil {
			return stats, fmt.Errorf("chunk %d failed: %w", i+1, err)
		}

		stats.record(time.Since(start), i, chunk)
		stats.pipeline.Add(pStats)

		after := streaming.TakeHeapSnapshot()
		obs := buildReplanObservation(i, chunk, before, after, aggSizeBefore, runner, chunks)
		newChunks := ap.Replan(obs)
		replanned := len(newChunks) != len(chunks)

		logChunkMemory(ctx, logger, ap, chunk, before, after, i, memBudget, replanned)

		if replanned {
			logger.InfoContext(ctx, "streaming: adaptive replan",
				"old_chunks", len(chunks), "new_chunks", len(newChunks),
				"ema_growth_kib", int64(ap.Stats().FinalGrowthRate)/streaming.KiB)
		}

		chunks = newChunks

		handleMemoryPressure(ctx, logger, after, memBudget)

		saveChunkCheckpoint(ctx, logger, runner, cpManager, checkpointables, commits, chunk, chunks, i, repoPath, analyzerNames)

		if onChunkComplete != nil {
			cbErr := onChunkComplete(runner)
			if cbErr != nil {
				return stats, fmt.Errorf("chunk %d: onChunkComplete: %w", i+1, cbErr)
			}
		}
	}

	return stats, nil
}

// processChunksFromIterator loads commits chunk-at-a-time from the iterator,
// processes each chunk, and frees the commits after processing. This keeps
// memory proportional to chunk size rather than total repository size.
func processChunksFromIterator( //nolint:funlen,gocognit // sequential chunk processing loop.
	ctx context.Context,
	logger *slog.Logger,
	runner *Runner,
	iter *gitlib.CommitIter,
	commitCount int,
	chunks []streaming.ChunkBounds,
	hibernatables []streaming.Hibernatable,
	checkpointables []checkpoint.Checkpointable,
	cpManager *checkpoint.Manager,
	repoPath string,
	analyzerNames []string,
	startChunk int,
	ap *streaming.AdaptivePlanner,
	memBudget int64,
	onChunkComplete func(runner *Runner) error,
) (chunkStats, error) {
	var stats chunkStats

	for i := startChunk; i < len(chunks); i++ {
		chunk := chunks[i]
		chunkSize := chunk.End - chunk.Start

		logger.InfoContext(ctx, "streaming[iter]: processing chunk",
			"chunk", i+1, "total", len(chunks), "start", chunk.Start, "end", chunk.End)

		if i > startChunk {
			hibErr := hibernateAndBoot(hibernatables)
			if hibErr != nil {
				return stats, hibErr
			}
		}

		// Load this chunk's commits from the iterator.
		chunkCommits, loadErr := loadCommitsFromIterator(iter, chunkSize)
		if loadErr != nil {
			return stats, fmt.Errorf("chunk %d: failed to load commits from iterator: %w", i+1, loadErr)
		}

		aggSizeBefore := runner.AggregatorStateSize()
		runner.ResetTCCount()

		before := streaming.TakeHeapSnapshot()

		start := time.Now()

		pStats, err := runner.ProcessChunk(ctx, chunkCommits, chunk.Start, i)
		if err != nil {
			freeCommits(chunkCommits)

			return stats, fmt.Errorf("chunk %d failed: %w", i+1, err)
		}

		stats.record(time.Since(start), i, chunk)
		stats.pipeline.Add(pStats)

		after := streaming.TakeHeapSnapshot()
		obs := buildReplanObservation(i, chunk, before, after, aggSizeBefore, runner, chunks)
		newChunks := ap.Replan(obs)
		replanned := len(newChunks) != len(chunks)

		logChunkMemory(ctx, logger, ap, chunk, before, after, i, memBudget, replanned)

		if replanned {
			logger.InfoContext(ctx, "streaming[iter]: adaptive replan",
				"old_chunks", len(chunks), "new_chunks", len(newChunks),
				"ema_growth_kib", int64(ap.Stats().FinalGrowthRate)/streaming.KiB)
		}

		chunks = newChunks

		saveIteratorCheckpoint(
			ctx, logger, runner, cpManager, checkpointables, chunkCommits, commitCount,
			chunk, chunks, i, repoPath, analyzerNames,
		)

		// Free all commits in this chunk — they are no longer needed.
		freeCommits(chunkCommits)

		handleMemoryPressure(ctx, logger, after, memBudget)

		if onChunkComplete != nil {
			cbErr := onChunkComplete(runner)
			if cbErr != nil {
				return stats, fmt.Errorf("chunk %d: onChunkComplete: %w", i+1, cbErr)
			}
		}
	}

	return stats, nil
}

// loadCommitsFromIterator reads n commits from the iterator into a slice.
// Returns an error if the iterator is exhausted before n commits are read.
func loadCommitsFromIterator(iter *gitlib.CommitIter, n int) ([]*gitlib.Commit, error) {
	commits := make([]*gitlib.Commit, 0, n)

	for range n {
		c, err := iter.Next()
		if err != nil {
			freeCommits(commits)

			return nil, fmt.Errorf("expected %d commits, got %d: %w", n, len(commits), err)
		}

		commits = append(commits, c)
	}

	return commits, nil
}

// freeCommits releases all commit resources in the slice.
func freeCommits(commits []*gitlib.Commit) {
	for _, c := range commits {
		c.Free()
	}
}

// doubleBufferBudgetDivisor is the factor by which available memory is divided
// when double-buffering is active (two chunks in flight simultaneously).
const doubleBufferBudgetDivisor = 2

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
	ap              *streaming.AdaptivePlanner
	memBudget       int64
	onChunkComplete func(runner *Runner) error
}

// processChunksDoubleBuffered overlaps chunk K+1's pipeline with chunk K's
// analyzer consumption. The first chunk runs normally (no prefetch available).
// For each subsequent chunk, the pipeline was started during the previous
// chunk's consumption, so data is immediately available.
func processChunksDoubleBuffered( //nolint:funlen,gocognit // sequential double-buffered chunk processing.
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
	ap *streaming.AdaptivePlanner,
	memBudget int64,
	onChunkComplete func(runner *Runner) error,
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
		ap:              ap,
		memBudget:       memBudget,
		onChunkComplete: onChunkComplete,
	}

	for idx := startChunk; idx < len(st.chunks); idx++ {
		// Save next chunk boundaries before prefetch so we can detect replan changes.
		prefetchedNext := st.safeNextChunk(idx)
		prefetch := st.startNextPrefetch(ctx, idx)

		aggSizeBefore := st.runner.AggregatorStateSize()
		st.runner.ResetTCCount()

		before := streaming.TakeHeapSnapshot()

		dur, pStats, err := st.processCurrentChunk(ctx, idx, startChunk)
		if err != nil {
			drainPrefetch(prefetch)

			return stats, err
		}

		stats.record(dur, idx, st.chunks[idx])
		stats.pipeline.Add(pStats)

		after := streaming.TakeHeapSnapshot()
		prefetch = st.replanAndDrainStale(ctx, idx, before, after, aggSizeBefore, prefetchedNext, prefetch)

		handleMemoryPressure(ctx, logger, after, st.memBudget)

		if st.onChunkComplete != nil {
			cbErr := st.onChunkComplete(st.runner)
			if cbErr != nil {
				drainPrefetch(prefetch)

				return stats, fmt.Errorf("chunk %d: onChunkComplete: %w", idx+1, cbErr)
			}
		}

		consumed, consumeDur, consumePStats, consumeErr := st.consumePrefetched(ctx, idx, prefetch)
		if consumeErr != nil {
			return stats, consumeErr
		}

		if consumed {
			stats.record(consumeDur, idx+1, st.chunks[idx+1])
			stats.pipeline.Add(consumePStats)

			if st.onChunkComplete != nil {
				cbErr := st.onChunkComplete(st.runner)
				if cbErr != nil {
					//nolint:mnd // idx+2 is the prefetched chunk number.
					return stats, fmt.Errorf("chunk %d: onChunkComplete: %w", idx+2, cbErr)
				}
			}

			idx++ // Skip the prefetched chunk in the loop.
		}
	}

	return stats, nil
}

// replanAndDrainStale runs three-metric adaptive replanning for the double-buffered
// loop and drains the stale prefetch if chunk boundaries changed. Returns the
// (possibly nil) prefetch channel to use for consumption.
func (st *doubleBufferState) replanAndDrainStale(
	ctx context.Context, idx int,
	before, after streaming.HeapSnapshot,
	aggSizeBefore int64,
	prefetchedNext streaming.ChunkBounds,
	prefetch <-chan prefetchedChunk,
) <-chan prefetchedChunk {
	obs := buildReplanObservation(idx, st.chunks[idx], before, after, aggSizeBefore, st.runner, st.chunks)
	newChunks := st.ap.Replan(obs)
	replanned := len(newChunks) != len(st.chunks)

	logChunkMemory(ctx, st.logger, st.ap, st.chunks[idx], before, after, idx, st.memBudget, replanned)

	if replanned {
		st.logger.InfoContext(ctx, "streaming[db]: adaptive replan",
			"old_chunks", len(st.chunks), "new_chunks", len(newChunks),
			"ema_growth_kib", int64(st.ap.Stats().FinalGrowthRate)/streaming.KiB)

		// If next chunk boundaries changed, drain stale prefetch.
		newNext := safeChunkAt(newChunks, idx+1)
		if prefetch != nil && !chunksEqual(prefetchedNext, newNext) {
			drainPrefetch(prefetch)
			prefetch = nil
		}
	}

	st.chunks = newChunks

	return prefetch
}

// safeNextChunk returns the chunk at idx+1, or a zero ChunkBounds if out of range.
func (st *doubleBufferState) safeNextChunk(idx int) streaming.ChunkBounds {
	return safeChunkAt(st.chunks, idx+1)
}

// safeChunkAt returns the chunk at index i, or a zero ChunkBounds if out of range.
func safeChunkAt(chunks []streaming.ChunkBounds, i int) streaming.ChunkBounds {
	if i < 0 || i >= len(chunks) {
		return streaming.ChunkBounds{}
	}

	return chunks[i]
}

// chunksEqual returns true if two chunk bounds are identical.
func chunksEqual(a, b streaming.ChunkBounds) bool {
	return a.Start == b.Start && a.End == b.End
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

	saveChunkCheckpoint(
		ctx, st.logger, st.runner, st.cpManager, st.checkpointables,
		st.commits, chunk, st.chunks, idx, st.repoPath, st.analyzerNames,
	)

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
		ctx, st.logger, st.runner, st.cpManager, st.checkpointables, st.commits,
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
// and this is not the last chunk). Aggregator in-memory state is flushed to disk before saving.
func saveChunkCheckpoint(
	ctx context.Context,
	logger *slog.Logger,
	runner *Runner,
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

	// Flush aggregator in-memory state to disk so spill files are complete.
	spillErr := runner.SpillAggregators()
	if spillErr != nil {
		logger.WarnContext(ctx, "failed to spill aggregators before checkpoint", "error", spillErr)
	}

	chunkCommits := commits[chunk.Start:chunk.End]
	lastCommit := chunkCommits[len(chunkCommits)-1]

	state := checkpoint.StreamingState{
		TotalCommits:     len(commits),
		ProcessedCommits: chunk.End,
		CurrentChunk:     chunkIdx,
		TotalChunks:      len(chunks),
		LastCommitHash:   lastCommit.Hash().String(),
		AggregatorSpills: runner.AggregatorSpills(),
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

// saveIteratorCheckpoint saves a checkpoint after processing a chunk in iterator
// mode, where only the current chunk's commits are available (not the full slice).
func saveIteratorCheckpoint(
	ctx context.Context,
	logger *slog.Logger,
	runner *Runner,
	cpManager *checkpoint.Manager,
	checkpointables []checkpoint.Checkpointable,
	chunkCommits []*gitlib.Commit,
	totalCommits int,
	chunk streaming.ChunkBounds,
	chunks []streaming.ChunkBounds,
	chunkIdx int,
	repoPath string,
	analyzerNames []string,
) {
	if cpManager == nil || chunkIdx >= len(chunks)-1 {
		return
	}

	spillErr := runner.SpillAggregators()
	if spillErr != nil {
		logger.WarnContext(ctx, "failed to spill aggregators before checkpoint", "error", spillErr)
	}

	lastCommit := chunkCommits[len(chunkCommits)-1]

	state := checkpoint.StreamingState{
		TotalCommits:     totalCommits,
		ProcessedCommits: chunk.End,
		CurrentChunk:     chunkIdx,
		TotalChunks:      len(chunks),
		LastCommitHash:   lastCommit.Hash().String(),
		AggregatorSpills: runner.AggregatorSpills(),
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

	return float64(hits) / float64(total) * percentScale
}

// buildReplanObservation computes the three-metric adaptive feedback observation
// from pre/post chunk heap snapshots, aggregator state delta, and TC count.
func buildReplanObservation(
	chunkIndex int, chunk streaming.ChunkBounds,
	before, after streaming.HeapSnapshot,
	aggSizeBefore int64, runner *Runner,
	currentChunks []streaming.ChunkBounds,
) streaming.ReplanObservation {
	commitsInChunk := int64(chunk.End - chunk.Start)
	if commitsInChunk <= 0 {
		return streaming.ReplanObservation{
			ChunkIndex:    chunkIndex,
			Chunk:         chunk,
			CurrentChunks: currentChunks,
		}
	}

	aggSizeAfter := runner.AggregatorStateSize()
	aggDelta := aggSizeAfter - aggSizeBefore

	// Use RSS delta when available (captures Go heap + native C memory from
	// libgit2/tree-sitter). Falls back to HeapInuse on non-Linux platforms.
	totalDelta := after.HeapInuse - before.HeapInuse
	if after.RSS > 0 && before.RSS > 0 {
		rssDelta := after.RSS - before.RSS
		if rssDelta > totalDelta {
			totalDelta = rssDelta
		}
	}

	// Work growth = total memory delta minus aggregator delta, per commit.
	workDelta := totalDelta - aggDelta
	workGrowth := workDelta / commitsInChunk

	// TC payload per commit: count * declared average size / commits.
	tcCount := runner.TCCountAccumulated()
	tcPayload := tcCount * streaming.DefaultAvgTCSize / commitsInChunk

	aggPerCommit := aggDelta / commitsInChunk

	return streaming.ReplanObservation{
		ChunkIndex:          chunkIndex,
		Chunk:               chunk,
		WorkGrowthPerCommit: workGrowth,
		TCPayloadPerCommit:  tcPayload,
		AggGrowthPerCommit:  aggPerCommit,
		CurrentChunks:       currentChunks,
	}
}

// logChunkMemory emits structured per-chunk memory telemetry via the streaming package.
func logChunkMemory(
	ctx context.Context, logger *slog.Logger,
	ap *streaming.AdaptivePlanner, chunk streaming.ChunkBounds,
	before, after streaming.HeapSnapshot,
	chunkIndex int, memBudget int64, replanned bool,
) {
	commitsInChunk := int64(chunk.End - chunk.Start)

	var growthPerCommit int64
	if commitsInChunk > 0 {
		growthPerCommit = (after.HeapInuse - before.HeapInuse) / commitsInChunk
	}

	// Use max(HeapInuse, RSS) for budget comparison — RSS captures native C
	// memory (libgit2, tree-sitter) that HeapInuse misses.
	effectiveUsage := max(after.HeapInuse, after.RSS)

	var budgetPct float64
	if memBudget > 0 {
		budgetPct = float64(effectiveUsage) * percentScale / float64(memBudget)
	}

	streaming.LogChunkMemory(ctx, logger, streaming.ChunkMemoryLog{
		ChunkIndex:      chunkIndex,
		HeapBefore:      before.HeapInuse,
		HeapAfter:       after.HeapInuse,
		SysAfter:        after.Sys,
		RSSAfter:        after.RSS,
		BudgetUsedPct:   budgetPct,
		GrowthPerCommit: growthPerCommit,
		EMAGrowthRate:   ap.Stats().FinalGrowthRate,
		Replanned:       replanned,
	})
}

// percentScale converts a fraction to a percentage.
const percentScale = 100.0

// handleMemoryPressure checks post-chunk heap usage against the budget and
// takes corrective action. At warning level (>80%), it logs a warning. At
// critical level (>90%), it forces an immediate GC + FreeOSMemory to reclaim
// memory before the next chunk starts.
func handleMemoryPressure(
	ctx context.Context, logger *slog.Logger,
	snapshot streaming.HeapSnapshot, memBudget int64,
) {
	// Use max(HeapInuse, RSS) for pressure detection — RSS captures native C
	// memory (libgit2, tree-sitter) that HeapInuse misses.
	effectiveUsage := max(snapshot.HeapInuse, snapshot.RSS)

	pressure := streaming.CheckMemoryPressure(effectiveUsage, memBudget)

	switch pressure {
	case streaming.PressureCritical:
		logger.WarnContext(ctx, "streaming: memory pressure critical, forcing GC",
			"heap_mib", snapshot.HeapInuse/streaming.MiB,
			"rss_mib", snapshot.RSS/streaming.MiB,
			"budget_mib", memBudget/streaming.MiB,
			"usage_pct", float64(effectiveUsage)*percentScale/float64(memBudget))

		runtime.GC()
		debug.FreeOSMemory()
		gitlib.ReleaseNativeMemory()

	case streaming.PressureWarning:
		logger.WarnContext(ctx, "streaming: memory pressure warning",
			"heap_mib", snapshot.HeapInuse/streaming.MiB,
			"rss_mib", snapshot.RSS/streaming.MiB,
			"budget_mib", memBudget/streaming.MiB,
			"usage_pct", float64(effectiveUsage)*percentScale/float64(memBudget))

	case streaming.PressureNone:
		// No action needed.
	}
}

func hibernateAndBoot(hibernatables []streaming.Hibernatable) error {
	for _, h := range hibernatables {
		err := h.Hibernate()
		if err != nil {
			return fmt.Errorf("hibernation failed: %w", err)
		}
	}

	// Force GC to collect memory freed by Hibernate/Spill, then release it
	// back to the OS. Without this, Go retains freed heap pages and RSS stays
	// high even after spilling data to disk.
	runtime.GC()
	debug.FreeOSMemory()

	// Release native C memory (glibc arenas) back to the OS. This complements
	// FreeOSMemory which only handles Go heap. Between chunks, libgit2 has
	// freed large amounts of native memory that glibc retains in arenas.
	gitlib.ReleaseNativeMemory()

	for _, h := range hibernatables {
		err := h.Boot()
		if err != nil {
			return fmt.Errorf("boot failed: %w", err)
		}
	}

	return nil
}
