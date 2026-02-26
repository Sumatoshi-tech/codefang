package framework

import (
	"bytes"
	"context"
	"fmt"
	"os"
	"reflect"
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

// defaultMemoryLimitBytes is the default soft memory limit (8 GiB).
// This caps peak memory usage while leaving headroom for the OS.
// Go's GC becomes aggressive as heap approaches this limit, preventing OOM.
const defaultMemoryLimitBytes = uint64(8 * 1024 * 1024 * 1024)

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

// PipelineStats holds cumulative pipeline metrics for a single Coordinator run.
// Populated during Process(); valid after the returned channel is fully drained.
type PipelineStats struct {
	BlobDuration time.Duration
	DiffDuration time.Duration
	UASTDuration time.Duration

	BlobCacheHits   int64
	BlobCacheMisses int64
	DiffCacheHits   int64
	DiffCacheMisses int64
}

// Add accumulates another PipelineStats into this one (cross-chunk aggregation).
func (s *PipelineStats) Add(other PipelineStats) {
	s.BlobDuration += other.BlobDuration
	s.DiffDuration += other.DiffDuration
	s.UASTDuration += other.UASTDuration
	s.BlobCacheHits += other.BlobCacheHits
	s.BlobCacheMisses += other.BlobCacheMisses
	s.DiffCacheHits += other.DiffCacheHits
	s.DiffCacheMisses += other.DiffCacheMisses
}

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

	// FirstParent indicates whether the history walk is restricted to the first parent.
	FirstParent bool

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

// Pipeline memory model constants matching pkg/budget/model.go.
// Duplicated here to avoid a circular dependency (budget imports framework).
const (
	runtimeOverhead      = 250 * 1024 * 1024 // Go runtime + libgit2 base + shared mmap.
	repoHandleSize       = 10 * 1024 * 1024  // Per-worker libgit2 handle (Go-visible).
	workerNativeOverhead = 50 * 1024 * 1024  // Per-worker C/mmap overhead from libgit2.
	avgDiffEntrySize     = 2 * 1024          // Average cached diff entry.
	avgCommitDataSize    = 64 * 1024         // Average in-flight commit data.
)

// EstimatedOverhead returns the estimated memory consumed by the pipeline
// infrastructure (runtime, workers, caches, buffers, native/mmap overhead) —
// everything except analyzer state. This allows the streaming planner to
// accurately compute how much memory remains for analyzer state growth.
func (c CoordinatorConfig) EstimatedOverhead() int64 {
	workers := int64(c.Workers) * (repoHandleSize + int64(c.BlobArenaSize) + workerNativeOverhead)
	caches := c.BlobCacheSize + int64(c.DiffCacheSize)*avgDiffEntrySize
	buffers := int64(c.BufferSize) * avgCommitDataSize

	return runtimeOverhead + workers + caches + buffers
}

// Coordinator orchestrates the full data processing pipeline.
type Coordinator struct {
	repo   *gitlib.Repository
	config CoordinatorConfig

	// stats holds cumulative pipeline metrics, populated after Process()
	// output channel is fully drained.
	stats PipelineStats

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

// Stats returns the pipeline stats collected during Process().
// Only valid after the channel returned by Process() is fully drained.
func (c *Coordinator) Stats() PipelineStats {
	return c.stats
}

func applyRuntimeTuning(config CoordinatorConfig, memBudgetOverride int64) []byte {
	applyGCPercent(config.GCPercent)

	if memBudgetOverride > 0 {
		applyMemoryLimitFromBudget(memBudgetOverride)
	} else {
		applyMemoryLimit()
	}

	return applyBallast(config.BallastSize)
}

// budgetLimitRatio is the fraction of the user's memory budget to use as the
// soft memory limit. Leaves headroom for the GC to operate without thrashing.
const budgetLimitRatio = 95

// systemRAMLimitRatio caps the soft memory limit at 90% of system RAM to
// prevent setting a limit higher than what the system can support.
const systemRAMLimitRatio = 90

// applyMemoryLimitFromBudget sets Go's soft memory limit to a fraction of the
// user's memory budget. Capped at 90% of system RAM to prevent GC thrashing
// when the budget exceeds available memory.
func applyMemoryLimitFromBudget(budget int64) {
	limit := resolveMemoryLimitFromBudget(budget, detectTotalMemoryBytes())
	debug.SetMemoryLimit(SafeInt64(limit))
}

func resolveMemoryLimitFromBudget(budget int64, totalMemoryBytes uint64) uint64 {
	budgetBased := uint64(budget) * budgetLimitRatio / percentDivisor

	if totalMemoryBytes > 0 {
		systemCap := totalMemoryBytes * systemRAMLimitRatio / percentDivisor

		return min(budgetBased, systemCap)
	}

	return budgetBased
}

// applyMemoryLimit sets Go's soft memory limit based on available system memory.
// Uses 75% of system memory (capped at 4 GiB) to trigger aggressive GC before OOM.
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
// After the returned channel is fully drained, call Stats() to retrieve
// pipeline timing and cache metrics.
func (c *Coordinator) Process(ctx context.Context, commits []*gitlib.Commit) <-chan CommitData {
	// Start all workers.
	c.seqWorker.Start()

	for _, w := range c.poolWorkers {
		w.Start()
	}

	// Pipeline: Commits -> Blobs -> Diffs -> [UAST].
	commitChan := c.commitStreamer.Stream(ctx, commits)

	blobHitsBefore, blobMissesBefore := cacheStats(c.blobCache)
	diffHitsBefore, diffMissesBefore := cacheStats(c.diffCache)

	blobStart := time.Now()
	blobOut, blobDone := signalOnDrain(c.blobPipeline.Process(ctx, commitChan))

	diffStart := time.Now()
	diffOut, diffDone := signalOnDrain(c.diffPipeline.Process(ctx, blobOut))

	// Optionally add UAST pipeline stage for pre-computed UAST parsing.
	var dataChan <-chan CommitData

	var uastDone <-chan struct{}

	var uastStart time.Time

	if c.uastPipeline != nil {
		uastStart = time.Now()

		var uastOut <-chan CommitData

		uastOut, uastDone = signalOnDrain(c.uastPipeline.Process(ctx, diffOut))

		dataChan = uastOut
	} else {
		dataChan = diffOut
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

		// All stages are complete. Record timing and cache deltas.
		c.recordStageTiming(blobDone, blobStart, diffDone, diffStart, uastDone, uastStart)
		c.recordCacheDeltas(blobHitsBefore, blobMissesBefore, diffHitsBefore, diffMissesBefore)

		// Cleanup: stop workers and free resources.
		c.stopWorkers()

		// Free pool repos.
		for _, r := range c.poolRepos {
			r.Free()
		}
	}()

	return finalChan
}

// recordStageTiming waits for each pipeline stage to finish and records its duration.
func (c *Coordinator) recordStageTiming(
	blobDone <-chan struct{}, blobStart time.Time,
	diffDone <-chan struct{}, diffStart time.Time,
	uastDone <-chan struct{}, uastStart time.Time,
) {
	<-blobDone

	c.stats.BlobDuration = time.Since(blobStart)

	<-diffDone

	c.stats.DiffDuration = time.Since(diffStart)

	if uastDone != nil {
		<-uastDone

		c.stats.UASTDuration = time.Since(uastStart)
	}
}

// recordCacheDeltas computes cache hit/miss deltas since the given baselines.
func (c *Coordinator) recordCacheDeltas(blobHitsBefore, blobMissesBefore, diffHitsBefore, diffMissesBefore int64) {
	if c.blobCache != nil {
		c.stats.BlobCacheHits = c.blobCache.CacheHits() - blobHitsBefore
		c.stats.BlobCacheMisses = c.blobCache.CacheMisses() - blobMissesBefore
	}

	if c.diffCache != nil {
		c.stats.DiffCacheHits = c.diffCache.CacheHits() - diffHitsBefore
		c.stats.DiffCacheMisses = c.diffCache.CacheMisses() - diffMissesBefore
	}
}

// stopWorkers closes request channels and stops all workers.
func (c *Coordinator) stopWorkers() {
	close(c.seqRequests)
	close(c.poolRequests)

	c.seqWorker.Stop()

	for _, w := range c.poolWorkers {
		w.Stop()
	}
}

// cacheStatsProvider can report cache hit/miss counters.
type cacheStatsProvider interface {
	CacheHits() int64
	CacheMisses() int64
}

// cacheStats returns the current hit/miss counters, or zero if cache is nil.
func cacheStats[T cacheStatsProvider](c T) (hits, misses int64) {
	if reflect.ValueOf(&c).Elem().IsNil() {
		return 0, 0
	}

	return c.CacheHits(), c.CacheMisses()
}

// signalOnDrain returns a channel that is closed after all items from src
// have been forwarded to dst. This enables ending stage spans independently.
func signalOnDrain[T any](src <-chan T) (forwarded <-chan T, drained <-chan struct{}) {
	sig := make(chan struct{})
	out := make(chan T)

	go func() {
		defer close(sig)
		defer close(out)

		for item := range src {
			out <- item
		}
	}()

	return out, sig
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
