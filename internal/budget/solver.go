package budget

import (
	"errors"
	"runtime"

	"github.com/Sumatoshi-tech/codefang/internal/framework"
	"github.com/Sumatoshi-tech/codefang/pkg/units"
)

// Allocation proportions for budget distribution.
const (
	// CacheAllocationPercent is the percentage of available budget for caches.
	CacheAllocationPercent = 60

	// WorkerAllocationPercent is the percentage of available budget for workers.
	WorkerAllocationPercent = 30

	// BufferAllocationPercent is the percentage of available budget for buffers.
	BufferAllocationPercent = 10

	// SlackPercent is reserved for runtime overhead.
	SlackPercent = 5

	// BlobCacheRatio is the portion of cache allocation for blob cache.
	BlobCacheRatio = 80

	// DiffCacheRatio is the portion of cache allocation for diff cache.
	DiffCacheRatio = 20

	// percentDivisor is used for percentage calculations.
	percentDivisor = 100
)

// Float64 weights derived from the integer percentage constants above.
// Used with allocateProportionally for budget distribution.
const (
	cacheWeight  = float64(CacheAllocationPercent) / percentDivisor
	workerWeight = float64(WorkerAllocationPercent) / percentDivisor
	bufferWeight = float64(BufferAllocationPercent) / percentDivisor
	blobWeight   = float64(BlobCacheRatio) / percentDivisor
	diffWeight   = float64(DiffCacheRatio) / percentDivisor
)

// Bucket name constants for allocateProportionally.
const (
	bucketCache  = "cache"
	bucketWorker = "worker"
	bucketBuffer = "buffer"
	bucketBlob   = "blob"
	bucketDiff   = "diff"
)

// Solver constraints.
const (
	// MinimumBudget is the smallest budget the solver will accept.
	// Must exceed BaseOverhead (250 MiB) plus room for at least 1 worker.
	MinimumBudget = 512 * units.MiB

	// DefaultArenaSize is the default blob arena size.
	// 8 MiB reduces fallback to per-blob C malloc (which accumulates in
	// glibc arenas as retained native RSS) by fitting ~97% of blob batches.
	DefaultArenaSize = 8 * units.MiB

	// MaxArenaSize is the maximum arena size allowed.
	MaxArenaSize = 16 * units.MiB

	// DefaultCommitBatchSize is used for all budget-derived configs.
	DefaultCommitBatchSize = 100

	// MinWorkers is the minimum number of workers.
	MinWorkers = 1

	// MinBufferSize is the minimum buffer size.
	MinBufferSize = 2

	// MinDiffCacheSize is the minimum diff cache entries.
	MinDiffCacheSize = 100

	// MinBlobCacheSize is the minimum blob cache size.
	MinBlobCacheSize = 1 * units.MiB

	// OptimalWorkerRatio is the percentage of CPU cores to use for workers.
	// Testing shows ~60% provides optimal performance due to contention overhead.
	OptimalWorkerRatio = 60

	// UASTPipelineWorkerRatio is the percentage of CPU cores for UAST pipeline workers.
	UASTPipelineWorkerRatio = 40

	// LeafWorkerDivisor controls leaf worker count: NumCPU / divisor.
	LeafWorkerDivisor = 3

	// MinLeafWorkers is the minimum number of leaf workers.
	MinLeafWorkers = 4
)

// Solver errors.
var (
	// ErrBudgetTooSmall indicates the budget is below the minimum required.
	ErrBudgetTooSmall = errors.New("memory budget is too small")
)

// allocateProportionally distributes total bytes across named buckets by weight.
// Weights must be in [0,1] and should sum to <= 1.0.
// Returns a map from bucket name to allocated bytes (truncated to int64).
func allocateProportionally(total int64, weights map[string]float64) map[string]int64 {
	result := make(map[string]int64, len(weights))

	for name, weight := range weights {
		result[name] = int64(float64(total) * weight)
	}

	return result
}

// SolveForBudget calculates optimal CoordinatorConfig for the given memory budget.
// The solver distributes available memory across workers, caches, and buffers
// while ensuring the total estimated usage stays within budget.
func SolveForBudget(budget int64) (framework.CoordinatorConfig, error) {
	if budget < MinimumBudget {
		return framework.CoordinatorConfig{}, ErrBudgetTooSmall
	}

	usableBudget := budget * (percentDivisor - SlackPercent) / percentDivisor
	available := usableBudget - BaseOverhead

	if available <= 0 {
		return framework.CoordinatorConfig{}, ErrBudgetTooSmall
	}

	allocs := allocateProportionally(available, map[string]float64{
		bucketCache:  cacheWeight,
		bucketWorker: workerWeight,
		bucketBuffer: bufferWeight,
	})

	cfg := deriveKnobs(allocs[bucketCache], allocs[bucketWorker], allocs[bucketBuffer])

	return cfg, nil
}

// deriveKnobs calculates individual configuration knobs from allocation budgets.
func deriveKnobs(cacheAlloc, workerAlloc, bufferAlloc int64) framework.CoordinatorConfig {
	// Workers: maximize within allocation, minimum 1, cap at optimal ratio of CPU cores.
	// Include native overhead (C/mmap) per worker in the cost calculation.
	maxWorkers := max(MinWorkers, runtime.NumCPU()*OptimalWorkerRatio/percentDivisor)
	workerCost := int64(RepoHandleSize + DefaultArenaSize + WorkerNativeOverhead)
	workers := max(MinWorkers, min(maxWorkers, int(workerAlloc/workerCost)))

	// Split cache allocation into blob and diff sub-budgets.
	cacheAllocs := allocateProportionally(cacheAlloc, map[string]float64{
		bucketBlob: blobWeight,
		bucketDiff: diffWeight,
	})

	// Blob cache: capped to avoid dominating the budget.
	blobCacheSize := max(int64(MinBlobCacheSize), cacheAllocs[bucketBlob])
	blobCacheSize = min(blobCacheSize, MaxBlobCacheSize)

	// Diff cache: converted to entries, capped.
	diffCacheAlloc := cacheAllocs[bucketDiff]
	diffCacheSize := max(MinDiffCacheSize, int(diffCacheAlloc/AvgDiffSize))
	diffCacheSize = min(diffCacheSize, MaxDiffCacheEntries)

	// Buffer size: based on allocation and workers.
	bufferSize := max(MinBufferSize, int(bufferAlloc/AvgCommitDataSize))

	// Use default arena size.
	arenaSize := DefaultArenaSize

	// UAST pipeline workers: use uastPipelineWorkerRatio of CPU cores.
	uastWorkers := max(1, runtime.NumCPU()*UASTPipelineWorkerRatio/percentDivisor)

	// Leaf workers: CPU / leafWorkerDivisor, with a floor.
	leafWorkers := max(MinLeafWorkers, runtime.NumCPU()/LeafWorkerDivisor)

	return framework.CoordinatorConfig{
		Workers:             workers,
		BufferSize:          bufferSize,
		CommitBatchSize:     DefaultCommitBatchSize,
		BlobCacheSize:       blobCacheSize,
		DiffCacheSize:       diffCacheSize,
		BlobArenaSize:       arenaSize,
		UASTPipelineWorkers: uastWorkers,
		LeafWorkers:         leafWorkers,
	}
}
