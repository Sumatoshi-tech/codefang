package framework

import (
	"context"
	"fmt"

	"runtime"

	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

// CoordinatorConfig configures the pipeline coordinator.
type CoordinatorConfig struct {
	// BatchConfig configures batch sizes for blob and diff operations.
	BatchConfig gitlib.BatchConfig

	// CommitBatchSize is the number of commits to process in each batch.
	CommitBatchSize int

	// Workers is the number of parallel workers for processing.
	Workers int

	// BufferSize is the size of internal channels.
	BufferSize int
}

// DefaultCoordinatorConfig returns the default coordinator configuration.
func DefaultCoordinatorConfig() CoordinatorConfig {
	workers := runtime.NumCPU()
	if workers < 1 {
		workers = 1
	}
	return CoordinatorConfig{
		BatchConfig:     gitlib.DefaultBatchConfig(),
		CommitBatchSize: 1, 
		Workers:         workers,
		BufferSize:      workers * 2, // Scale buffer with workers to keep them fed
	}
}

// Coordinator orchestrates the full data processing pipeline.
type Coordinator struct {
	repo   *gitlib.Repository
	config CoordinatorConfig

	commitStreamer *CommitStreamer
	blobPipeline   *BlobPipeline
	diffPipeline   *DiffPipeline
	
	// Workers
	seqWorker   *gitlib.Worker
	poolWorkers []*gitlib.Worker
	poolRepos   []*gitlib.Repository
	
	// Channels
	seqRequests  chan gitlib.WorkerRequest
	poolRequests chan gitlib.WorkerRequest
}

// NewCoordinator creates a new coordinator for the repository.
func NewCoordinator(repo *gitlib.Repository, config CoordinatorConfig) *Coordinator {
	if config.CommitBatchSize <= 0 {
		config.CommitBatchSize = 1
	}
	if config.BufferSize <= 0 {
		config.BufferSize = 10
	}
	if config.Workers <= 0 {
		config.Workers = 1
	}

	seqChan := make(chan gitlib.WorkerRequest, config.BufferSize)
	poolChan := make(chan gitlib.WorkerRequest, config.BufferSize*config.Workers)

	// Sequential worker uses the main repo (for commit stream + tree diffs)
	seqWorker := gitlib.NewWorker(repo, seqChan)

	// Pool workers use NEW repo handles
	poolWorkers := make([]*gitlib.Worker, config.Workers)
	poolRepos := make([]*gitlib.Repository, config.Workers)
	
	for i := 0; i < config.Workers; i++ {
		// Clone repo handle
		newRepo, err := gitlib.OpenRepository(repo.Path())
		if err != nil {
			panic(fmt.Errorf("failed to open repo for worker: %w", err))
		}
		poolRepos[i] = newRepo
		poolWorkers[i] = gitlib.NewWorker(newRepo, poolChan)
	}
	
	return &Coordinator{
		repo:   repo,
		config: config,
		commitStreamer: &CommitStreamer{
			BatchSize: config.CommitBatchSize,
			Lookahead: config.BufferSize,
		},
		blobPipeline: NewBlobPipeline(seqChan, poolChan, config.BufferSize),
		diffPipeline: NewDiffPipeline(poolChan, config.BufferSize),
		
		seqWorker:    seqWorker,
		poolWorkers:  poolWorkers,
		poolRepos:    poolRepos,
		seqRequests:  seqChan,
		poolRequests: poolChan,
	}
}

// Process runs the full pipeline on a slice of commits.
func (c *Coordinator) Process(ctx context.Context, commits []*gitlib.Commit) <-chan CommitData {
	// Start all workers
	c.seqWorker.Start()
	for _, w := range c.poolWorkers {
		w.Start()
	}

	// Pipeline: Commits -> Blobs -> Diffs
	commitChan := c.commitStreamer.Stream(ctx, commits)
	blobChan := c.blobPipeline.Process(ctx, commitChan)
	diffChan := c.diffPipeline.Process(ctx, blobChan)

	// Ensure workers stop when pipeline is done
	finalChan := make(chan CommitData)
	go func() {
		defer close(finalChan)
		
		// Wait for all data to pass through
		for data := range diffChan {
			select {
			case finalChan <- data:
			case <-ctx.Done():
				// Drain?
			}
		}

		// Cleanup
		// Close request channels to stop workers
		close(c.seqRequests)
		close(c.poolRequests)
		
		c.seqWorker.Stop()
		for _, w := range c.poolWorkers {
			w.Stop()
		}
		
		// Free pool repos
		for _, r := range c.poolRepos {
			r.Free()
		}
	}()

	return finalChan
}

// ProcessSingle processes a single commit.
func (c *Coordinator) ProcessSingle(ctx context.Context, commit *gitlib.Commit, index int) CommitData {
	commits := []*gitlib.Commit{commit}
	ch := c.Process(ctx, commits)
	return <-ch
}
