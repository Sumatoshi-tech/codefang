package framework

import (
	"context"
	"fmt"
	"log"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/checkpoint"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
	"github.com/Sumatoshi-tech/codefang/pkg/streaming"
)

// StreamingConfig holds configuration for streaming pipeline execution.
type StreamingConfig struct {
	MemBudget     int64
	Checkpoint    CheckpointParams
	RepoPath      string
	AnalyzerNames []string
}

// RunStreaming executes the pipeline in streaming chunks with optional checkpoint support.
// When the memory budget is sufficient and multiple chunks are needed, it
// enables double-buffered chunk pipelining to overlap pipeline execution
// with analyzer consumption.
func RunStreaming(
	runner *Runner,
	commits []*gitlib.Commit,
	analyzers []analyze.HistoryAnalyzer,
	config StreamingConfig,
) (map[analyze.HistoryAnalyzer]analyze.Report, error) {
	chunks, useDoubleBuffer := planChunksWithDoubleBuffer(len(commits), config.MemBudget)
	hibernatables := collectHibernatables(analyzers)
	checkpointables := collectCheckpointables(analyzers)

	cpManager := initCheckpointManager(config.Checkpoint, config.RepoPath, len(analyzers), len(checkpointables))

	log.Printf("streaming: processing %d commits in %d chunks (double-buffer=%t)",
		len(commits), len(chunks), useDoubleBuffer)

	startChunk := resolveStartChunk(cpManager, checkpointables, config)

	if startChunk == 0 {
		err := runner.Initialize()
		if err != nil {
			return nil, fmt.Errorf("initialization failed: %w", err)
		}
	}

	var err error

	if useDoubleBuffer {
		err = processChunksDoubleBuffered(
			runner, commits, chunks, hibernatables, checkpointables,
			cpManager, config.RepoPath, config.AnalyzerNames, startChunk,
		)
	} else {
		err = processChunksWithCheckpoint(
			runner, commits, chunks, hibernatables, checkpointables,
			cpManager, config.RepoPath, config.AnalyzerNames, startChunk,
		)
	}

	if err != nil {
		return nil, err
	}

	if cpManager != nil {
		clearErr := cpManager.Clear()
		if clearErr != nil {
			log.Printf("warning: failed to clear checkpoint after completion: %v", clearErr)
		}
	}

	return runner.Finalize()
}

// planChunksWithDoubleBuffer plans chunk boundaries, enabling double-buffering
// when memory budget allows. When double-buffering is active, the budget is
// halved to accommodate two concurrent chunks, which may result in more (smaller)
// chunks but the pipeline overlap compensates.
func planChunksWithDoubleBuffer(commitCount int, memBudget int64) ([]streaming.ChunkBounds, bool) {
	// First pass: plan with full budget to determine chunk count.
	initialChunks := planChunks(commitCount, memBudget)

	if !canDoubleBuffer(memBudget, len(initialChunks)) {
		return initialChunks, false
	}

	// Re-plan with halved budget for double-buffering.
	dbBudget := doubleBufferMemoryBudget(memBudget)
	dbChunks := planChunks(commitCount, dbBudget)

	return dbChunks, true
}

// initCheckpointManager creates and validates a checkpoint manager, returning nil if
// checkpointing is disabled or the analyzer set doesn't fully support it.
func initCheckpointManager(cpConfig CheckpointParams, repoPath string, totalAnalyzers, checkpointableCount int) *checkpoint.Manager {
	if !cpConfig.Enabled {
		return nil
	}

	repoHash := checkpoint.RepoHash(repoPath)
	cpManager := checkpoint.NewManager(cpConfig.Dir, repoHash)

	if cpConfig.ClearPrev {
		clearErr := cpManager.Clear()
		if clearErr != nil {
			log.Printf("warning: failed to clear checkpoint: %v", clearErr)
		}
	}

	if !CanResumeWithCheckpoint(totalAnalyzers, checkpointableCount) {
		log.Printf(
			"checkpoint: disabled; checkpoint support is incomplete (%d/%d analyzers)",
			checkpointableCount,
			totalAnalyzers,
		)

		return nil
	}

	return cpManager
}

// resolveStartChunk determines which chunk to start from, attempting checkpoint
// resume if configured and available.
func resolveStartChunk(cpManager *checkpoint.Manager, checkpointables []checkpoint.Checkpointable, config StreamingConfig) int {
	if cpManager == nil || !config.Checkpoint.Resume || !cpManager.Exists() {
		return 0
	}

	resumedChunk, err := tryResumeFromCheckpoint(cpManager, checkpointables, config.RepoPath, config.AnalyzerNames)
	if err != nil {
		log.Printf("checkpoint: resume failed, starting fresh: %v", err)

		return 0
	}

	if resumedChunk > 0 {
		log.Printf("checkpoint: resuming from chunk %d", resumedChunk+1)
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

func planChunks(commitCount int, memBudget int64) []streaming.ChunkBounds {
	planner := streaming.Planner{
		TotalCommits: commitCount,
		MemoryBudget: memBudget,
	}

	return planner.Plan()
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
	runner *Runner,
	commits []*gitlib.Commit,
	chunks []streaming.ChunkBounds,
	hibernatables []streaming.Hibernatable,
	checkpointables []checkpoint.Checkpointable,
	cpManager *checkpoint.Manager,
	repoPath string,
	analyzerNames []string,
	startChunk int,
) error {
	for i := startChunk; i < len(chunks); i++ {
		chunk := chunks[i]
		log.Printf("streaming: processing chunk %d/%d (commits %d-%d)",
			i+1, len(chunks), chunk.Start, chunk.End)

		if i > startChunk {
			hibErr := hibernateAndBoot(hibernatables)
			if hibErr != nil {
				return hibErr
			}
		}

		chunkCommits := commits[chunk.Start:chunk.End]

		err := runner.ProcessChunk(chunkCommits, chunk.Start)
		if err != nil {
			return fmt.Errorf("chunk %d failed: %w", i+1, err)
		}

		if cpManager != nil && i < len(chunks)-1 {
			lastCommit := chunkCommits[len(chunkCommits)-1]
			state := checkpoint.StreamingState{
				TotalCommits:     len(commits),
				ProcessedCommits: chunk.End,
				CurrentChunk:     i,
				TotalChunks:      len(chunks),
				LastCommitHash:   lastCommit.Hash().String(),
			}

			saveErr := cpManager.Save(checkpointables, state, repoPath, analyzerNames)
			if saveErr != nil {
				log.Printf("warning: failed to save checkpoint: %v", saveErr)
			} else {
				log.Printf("checkpoint: saved after chunk %d", i+1)
			}
		}
	}

	return nil
}

// doubleBufferBudgetDivisor is the factor by which available memory is divided
// when double-buffering is active (two chunks in flight simultaneously).
const doubleBufferBudgetDivisor = 2

// minDoubleBufferAvailable is the minimum available memory (after overhead)
// required to enable double-buffering. Each slot needs enough room for at
// least MinChunkSize commits of state growth.
const minDoubleBufferAvailable = int64(doubleBufferBudgetDivisor * streaming.MinChunkSize * streaming.AvgStateGrowthPerCommit)

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
	data []CommitData
	err  error
}

// prefetchPipeline opens a fresh repo handle, runs the Coordinator pipeline
// for the given commits, and collects all CommitData into a prefetchedChunk.
// The caller does not need to close the repo; it is freed internally.
func prefetchPipeline(repoPath string, config CoordinatorConfig, commits []*gitlib.Commit) prefetchedChunk {
	repo, openErr := gitlib.OpenRepository(repoPath)
	if openErr != nil {
		return prefetchedChunk{err: fmt.Errorf("prefetch: open repository: %w", openErr)}
	}

	ctx := context.Background()
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

	return prefetchedChunk{data: collected}
}

// startPrefetch launches prefetchPipeline in a background goroutine and
// returns a channel that delivers the result exactly once.
func startPrefetch(repoPath string, config CoordinatorConfig, commits []*gitlib.Commit) <-chan prefetchedChunk {
	ch := make(chan prefetchedChunk, 1)

	go func() {
		ch <- prefetchPipeline(repoPath, config, commits)

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
}

// processChunksDoubleBuffered overlaps chunk K+1's pipeline with chunk K's
// analyzer consumption. The first chunk runs normally (no prefetch available).
// For each subsequent chunk, the pipeline was started during the previous
// chunk's consumption, so data is immediately available.
func processChunksDoubleBuffered(
	runner *Runner,
	commits []*gitlib.Commit,
	chunks []streaming.ChunkBounds,
	hibernatables []streaming.Hibernatable,
	checkpointables []checkpoint.Checkpointable,
	cpManager *checkpoint.Manager,
	repoPath string,
	analyzerNames []string,
	startChunk int,
) error {
	st := &doubleBufferState{
		runner:          runner,
		commits:         commits,
		chunks:          chunks,
		hibernatables:   hibernatables,
		checkpointables: checkpointables,
		cpManager:       cpManager,
		repoPath:        repoPath,
		analyzerNames:   analyzerNames,
	}

	for idx := startChunk; idx < len(chunks); idx++ {
		prefetch := st.startNextPrefetch(idx)

		err := st.processCurrentChunk(idx, startChunk)
		if err != nil {
			drainPrefetch(prefetch)

			return err
		}

		consumed, consumeErr := st.consumePrefetched(idx, prefetch)
		if consumeErr != nil {
			return consumeErr
		}

		if consumed {
			idx++ // Skip the prefetched chunk in the loop.
		}
	}

	return nil
}

// startNextPrefetch begins pipeline execution for chunk idx+1 in a goroutine.
// Returns nil if there is no next chunk.
func (st *doubleBufferState) startNextPrefetch(idx int) <-chan prefetchedChunk {
	nextIdx := idx + 1
	if nextIdx >= len(st.chunks) {
		return nil
	}

	nextChunk := st.chunks[nextIdx]
	nextCommits := st.commits[nextChunk.Start:nextChunk.End]

	return startPrefetch(st.repoPath, st.runner.Config, nextCommits)
}

// processCurrentChunk hibernates (if not the first chunk), runs the pipeline
// through the Coordinator, and saves a checkpoint.
func (st *doubleBufferState) processCurrentChunk(idx, startChunk int) error {
	chunk := st.chunks[idx]
	chunkCommits := st.commits[chunk.Start:chunk.End]

	log.Printf("streaming[db]: processing chunk %d/%d (commits %d-%d)",
		idx+1, len(st.chunks), chunk.Start, chunk.End)

	if idx > startChunk {
		hibErr := hibernateAndBoot(st.hibernatables)
		if hibErr != nil {
			return hibErr
		}
	}

	processErr := st.runner.ProcessChunk(chunkCommits, chunk.Start)
	if processErr != nil {
		return fmt.Errorf("chunk %d failed: %w", idx+1, processErr)
	}

	saveChunkCheckpoint(st.cpManager, st.checkpointables, st.commits, chunk, st.chunks, idx, st.repoPath, st.analyzerNames)

	return nil
}

// consumePrefetched waits for the prefetched result, hibernates, and feeds it
// to analyzers. Returns true if a prefetched chunk was consumed.
func (st *doubleBufferState) consumePrefetched(idx int, prefetch <-chan prefetchedChunk) (bool, error) {
	if prefetch == nil {
		return false, nil
	}

	pf := <-prefetch

	nextIdx := idx + 1
	nextChunk := st.chunks[nextIdx]

	if pf.err != nil {
		return false, fmt.Errorf("prefetch chunk %d failed: %w", nextIdx+1, pf.err)
	}

	log.Printf("streaming[db]: consuming prefetched chunk %d/%d (commits %d-%d)",
		nextIdx+1, len(st.chunks), nextChunk.Start, nextChunk.End)

	hibErr := hibernateAndBoot(st.hibernatables)
	if hibErr != nil {
		return false, hibErr
	}

	processErr := st.runner.ProcessChunkFromData(pf.data, nextChunk.Start)
	if processErr != nil {
		return false, fmt.Errorf("chunk %d failed: %w", nextIdx+1, processErr)
	}

	saveChunkCheckpoint(st.cpManager, st.checkpointables, st.commits, nextChunk, st.chunks, nextIdx, st.repoPath, st.analyzerNames)

	return true, nil
}

// drainPrefetch waits for a pending prefetch to complete (if any) to prevent
// goroutine leaks. The result is discarded.
func drainPrefetch(ch <-chan prefetchedChunk) {
	if ch == nil {
		return
	}

	<-ch
}

// saveChunkCheckpoint saves a checkpoint after processing a chunk (if checkpointing is enabled
// and this is not the last chunk).
func saveChunkCheckpoint(
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
		log.Printf("warning: failed to save checkpoint: %v", saveErr)
	} else {
		log.Printf("checkpoint: saved after chunk %d", chunkIdx+1)
	}
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
