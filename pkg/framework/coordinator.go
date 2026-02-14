package framework

import (
	"bytes"
	"context"
	"fmt"
	"os"
	"runtime"
	"runtime/debug"
	"strconv"
	"time"

	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
	"github.com/Sumatoshi-tech/codefang/pkg/uast"
)

// bufferSizeMultiplier is the factor by which buffer size scales with worker count.
const bufferSizeMultiplier = 2

// optimalWorkerRatio is the fraction of CPU cores to use for workers.
// Testing shows ~60% of CPU cores provides optimal performance due to
// contention overhead when using all cores.
const optimalWorkerRatio = 60

// uastPipelineWorkerRatio is the fraction of CPU cores to use for UAST pipeline workers.
const uastPipelineWorkerRatio = 40

// leafWorkerDivisor controls the default number of leaf workers: NumCPU / divisor.
const leafWorkerDivisor = 3

// minLeafWorkers is the minimum number of leaf workers when enabled.
const minLeafWorkers = 4

// defaultCommitBatchSize is the default number of commits to process per batch.
const defaultCommitBatchSize = 100

// defaultBlobArenaBytes is the default arena size for blob loading (4 MB).
// Smaller arenas reduce GC pressure and improve cache locality.
const defaultBlobArenaBytes = 4 * 1024 * 1024

// minMemInfoFields is the minimum number of space-separated fields expected
// in a /proc/meminfo line (e.g. "MemTotal: 16384 kB" has 3 fields).
const minMemInfoFields = 2

// defaultMemoryLimitBytes is the default soft memory limit (14 GiB).
// This caps peak memory usage while leaving headroom for the OS.
// Go's GC becomes aggressive as heap approaches this limit, preventing OOM.
const defaultMemoryLimitBytes = uint64(14 * 1024 * 1024 * 1024)

// memoryLimitRatio is the fraction of system memory to use as the soft limit.
// Go's runtime GC becomes aggressive as heap approaches this limit.
const memoryLimitRatio = 75

// percentDivisor converts percentage ratios (e.g. 60, 75) to fractions.
const percentDivisor = 100

const (
	procMemInfoPath = "/proc/meminfo"
	memTotalPrefix  = "MemTotal:"
	memTotalUnitKiB = "kB"
	kibibyte        = uint64(1024)
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

	// BlobCacheSize is the maximum size of the global blob cache in bytes.
	// Set to 0 to disable caching.
	BlobCacheSize int64

	// DiffCacheSize is the maximum number of diff results to cache.
	// Set to 0 to disable caching.
	DiffCacheSize int

	// BlobArenaSize is the size of the memory arena for blob loading.
	// Defaults to 16MB if 0.
	BlobArenaSize int

	// UASTPipelineWorkers is the number of goroutines for parallel UAST parsing
	// in the pipeline stage. Set to 0 to disable the UAST pipeline stage.
	UASTPipelineWorkers int

	// LeafWorkers is the number of goroutines for parallel leaf analyzer consumption.
	// Each worker processes a disjoint subset of commits via Fork/Merge.
	// Set to 0 to disable parallel leaf consumption (serial path).
	LeafWorkers int

	// GCPercent controls Go's GC aggressiveness.
	// Set to 0 to use auto mode (200 when system memory > 32 GiB).
	GCPercent int

	// BallastSize reserves bytes in a long-lived slice to smooth GC behavior.
	// Set to 0 to disable ballast allocation.
	BallastSize int64

	// WorkerTimeout is the maximum time to wait for a worker response before
	// considering it stalled. Set to 0 to disable the watchdog.
	WorkerTimeout time.Duration
}

// DefaultCoordinatorConfig returns the default coordinator configuration.
func DefaultCoordinatorConfig() CoordinatorConfig {
	// Use 60% of CPU cores for optimal performance.
	// Testing on kubernetes repo (135k commits) showed that using all CPU cores
	// causes contention and degrades performance by ~17% compared to 60%.
	workers := max(runtime.NumCPU()*optimalWorkerRatio/percentDivisor, 1)

	uastWorkers := max(runtime.NumCPU()*uastPipelineWorkerRatio/percentDivisor, 1)
	leafWorkers := max(runtime.NumCPU()/leafWorkerDivisor, minLeafWorkers)

	return CoordinatorConfig{
		BatchConfig:         gitlib.DefaultBatchConfig(),
		CommitBatchSize:     defaultCommitBatchSize,
		Workers:             workers,
		BufferSize:          workers * bufferSizeMultiplier, // Scale buffer with workers to keep them fed.
		BlobCacheSize:       DefaultGlobalCacheSize,
		DiffCacheSize:       DefaultDiffCacheSize,
		UASTPipelineWorkers: uastWorkers,
		LeafWorkers:         leafWorkers,
		BlobArenaSize:       defaultBlobArenaBytes,
		GCPercent:           0,
		BallastSize:         0,
	}
}

// Coordinator orchestrates the full data processing pipeline.
type Coordinator struct {
	repo   *gitlib.Repository
	config CoordinatorConfig

	commitStreamer *CommitStreamer
	blobPipeline   *BlobPipeline
	diffPipeline   *DiffPipeline
	uastPipeline   *UASTPipeline
	blobCache      *GlobalBlobCache
	diffCache      *DiffCache

	// Workers.
	seqWorker   *gitlib.Worker
	poolWorkers []*gitlib.Worker
	poolRepos   []*gitlib.Repository

	// Channels.
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

	// Sequential worker uses the main repo (for commit stream + tree diffs).
	seqWorker := gitlib.NewWorker(repo, seqChan)

	// Pool workers use NEW repo handles.
	poolWorkers := make([]*gitlib.Worker, config.Workers)
	poolRepos := make([]*gitlib.Repository, config.Workers)

	for i := range config.Workers {
		// Clone repo handle.
		newRepo, err := gitlib.OpenRepository(repo.Path())
		if err != nil {
			panic(fmt.Errorf("failed to open repo for worker: %w", err))
		}

		poolRepos[i] = newRepo
		poolWorkers[i] = gitlib.NewWorker(newRepo, poolChan)
	}

	// Create blob cache if configured.
	var blobCache *GlobalBlobCache
	if config.BlobCacheSize > 0 {
		blobCache = NewGlobalBlobCache(config.BlobCacheSize)
	}

	// Create diff cache if configured.
	var diffCache *DiffCache
	if config.DiffCacheSize > 0 {
		diffCache = NewDiffCache(config.DiffCacheSize)
	}

	blobPipeline := NewBlobPipelineWithCache(seqChan, poolChan, config.BufferSize, config.Workers, blobCache)
	if config.BlobArenaSize > 0 {
		blobPipeline.ArenaSize = config.BlobArenaSize
	}

	// Create UAST pipeline if workers are configured.
	var uastPipeline *UASTPipeline

	if config.UASTPipelineWorkers > 0 {
		parser, err := uast.NewParser()
		if err == nil {
			uastPipeline = NewUASTPipeline(parser, config.UASTPipelineWorkers, config.BufferSize)
		}
	}

	return &Coordinator{
		repo:   repo,
		config: config,
		commitStreamer: &CommitStreamer{
			BatchSize: config.CommitBatchSize,
			Lookahead: config.BufferSize,
		},
		blobPipeline: blobPipeline,
		diffPipeline: NewDiffPipelineWithCache(poolChan, config.BufferSize, diffCache),
		uastPipeline: uastPipeline,
		blobCache:    blobCache,
		diffCache:    diffCache,

		seqWorker:    seqWorker,
		poolWorkers:  poolWorkers,
		poolRepos:    poolRepos,
		seqRequests:  seqChan,
		poolRequests: poolChan,
	}
}

func applyRuntimeTuning(config CoordinatorConfig) []byte {
	applyGCPercent(config.GCPercent)
	applyMemoryLimit()

	return applyBallast(config.BallastSize)
}

// applyMemoryLimit sets Go's soft memory limit based on available system memory.
// Uses 75% of system memory (capped at 14 GiB) to trigger aggressive GC before OOM.
// Go's GC uses this as a target: when heap approaches the limit, GC runs more
// frequently regardless of GOGC. This prevents OOM on large analysis workloads.
func applyMemoryLimit() {
	limit := resolveMemoryLimit(detectTotalMemoryBytes())
	debug.SetMemoryLimit(SafeInt64(limit))
}

func resolveMemoryLimit(totalMemoryBytes uint64) uint64 {
	if totalMemoryBytes == 0 {
		return defaultMemoryLimitBytes
	}

	systemBased := totalMemoryBytes * memoryLimitRatio / percentDivisor

	return min(systemBased, defaultMemoryLimitBytes)
}

func applyGCPercent(requestedGCPercent int) {
	if requestedGCPercent <= 0 {
		return
	}

	debug.SetGCPercent(requestedGCPercent)
}

func applyBallast(ballastSize int64) []byte {
	if ballastSize <= 0 {
		return nil
	}

	return make([]byte, ballastSize)
}

func detectTotalMemoryBytes() uint64 {
	if runtime.GOOS != "linux" {
		return 0
	}

	memInfoBytes, err := os.ReadFile(procMemInfoPath)
	if err != nil {
		return 0
	}

	return parseMemTotalBytes(memInfoBytes)
}

func parseMemTotalBytes(memInfo []byte) uint64 {
	for line := range bytes.SplitSeq(memInfo, []byte{'\n'}) {
		if !bytes.HasPrefix(line, []byte(memTotalPrefix)) {
			continue
		}

		fields := bytes.Fields(line)
		if len(fields) < minMemInfoFields {
			return 0
		}

		memTotal, err := strconv.ParseUint(string(fields[1]), 10, 64)
		if err != nil {
			return 0
		}

		unit := memTotalUnitKiB
		if len(fields) > minMemInfoFields {
			unit = string(fields[2])
		}

		return scaleBytesByUnit(memTotal, unit)
	}

	return 0
}

func scaleBytesByUnit(value uint64, unit string) uint64 {
	switch unit {
	case memTotalUnitKiB:
		return value * kibibyte
	default:
		return value
	}
}

// Process runs the full pipeline on a slice of commits.
func (c *Coordinator) Process(ctx context.Context, commits []*gitlib.Commit) <-chan CommitData {
	// Start all workers.
	c.seqWorker.Start()

	for _, w := range c.poolWorkers {
		w.Start()
	}

	// Pipeline: Commits -> Blobs -> Diffs -> [UAST].
	commitChan := c.commitStreamer.Stream(ctx, commits)
	blobChan := c.blobPipeline.Process(ctx, commitChan)
	diffChan := c.diffPipeline.Process(ctx, blobChan)

	// Optionally add UAST pipeline stage for pre-computed UAST parsing.
	var dataChan <-chan CommitData
	if c.uastPipeline != nil {
		dataChan = c.uastPipeline.Process(ctx, diffChan)
	} else {
		dataChan = diffChan
	}

	// Ensure workers stop when pipeline is done.
	finalChan := make(chan CommitData)

	go func() {
		defer close(finalChan)

		// Wait for all data to pass through.
		for data := range dataChan {
			select {
			case finalChan <- data:
			case <-ctx.Done():
				// Drain?
			}
		}

		// Cleanup
		// Close request channels to stop workers.
		close(c.seqRequests)
		close(c.poolRequests)

		c.seqWorker.Stop()

		for _, w := range c.poolWorkers {
			w.Stop()
		}

		// Free pool repos.
		for _, r := range c.poolRepos {
			r.Free()
		}
	}()

	return finalChan
}

// ProcessSingle processes a single commit.
func (c *Coordinator) ProcessSingle(ctx context.Context, commit *gitlib.Commit, _ int) CommitData {
	commits := []*gitlib.Commit{commit}
	ch := c.Process(ctx, commits)

	return <-ch
}

// Config returns the coordinator configuration.
func (c *Coordinator) Config() CoordinatorConfig {
	return c.config
}
