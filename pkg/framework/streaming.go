package framework

import (
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
func RunStreaming(
	runner *Runner,
	commits []*gitlib.Commit,
	analyzers []analyze.HistoryAnalyzer,
	config StreamingConfig,
) (map[analyze.HistoryAnalyzer]analyze.Report, error) {
	chunks := planChunks(len(commits), config.MemBudget)
	hibernatables := collectHibernatables(analyzers)
	checkpointables := collectCheckpointables(analyzers)

	cpManager := initCheckpointManager(config.Checkpoint, config.RepoPath, len(analyzers), len(checkpointables))

	log.Printf("streaming: processing %d commits in %d chunks", len(commits), len(chunks))

	startChunk := resolveStartChunk(cpManager, checkpointables, config)

	if startChunk == 0 {
		err := runner.Initialize()
		if err != nil {
			return nil, fmt.Errorf("initialization failed: %w", err)
		}
	}

	err := processChunksWithCheckpoint(
		runner, commits, chunks, hibernatables, checkpointables,
		cpManager, config.RepoPath, config.AnalyzerNames, startChunk,
	)
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
