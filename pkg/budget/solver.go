package budget

import (
	"errors"
	"runtime"

	"github.com/Sumatoshi-tech/codefang/pkg/framework"
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

// Solver constraints.
const (
	// MinimumBudget is the smallest budget the solver will accept.
	// Must exceed BaseOverhead (250 MiB) plus room for at least 1 worker.
	MinimumBudget = 512 * MiB

	// DefaultArenaSize is the default blob arena size.
	DefaultArenaSize = 4 * MiB

	// MaxArenaSize is the maximum arena size allowed.
	MaxArenaSize = 16 * MiB

	// DefaultCommitBatchSize is used for all budget-derived configs.
	DefaultCommitBatchSize = 100

	// MinWorkers is the minimum number of workers.
	MinWorkers = 1

	// MinBufferSize is the minimum buffer size.
	MinBufferSize = 2

	// MinDiffCacheSize is the minimum diff cache entries.
	MinDiffCacheSize = 100

	// MinBlobCacheSize is the minimum blob cache size.
	MinBlobCacheSize = 1 * MiB

	// OptimalWorkerRatio is the percentage of CPU cores to use for workers.
	// Testing shows ~60% provides optimal performance due to contention overhead.
	OptimalWorkerRatio = 60
)

// Solver errors.
var (
	// ErrBudgetTooSmall indicates the budget is below the minimum required.
	ErrBudgetTooSmall = errors.New("memory budget is too small")
)

// SolveForBudget calculates optimal CoordinatorConfig for the given memory budget.
// The solver distributes available memory across workers, caches, and buffers
// while ensuring the total estimated usage stays within budget.
func SolveForBudget(budget int64) (framework.CoordinatorConfig, error) {
	if budget < MinimumBudget {
		return framework.CoordinatorConfig{}, ErrBudgetTooSmall
	}

	// Reserve slack for runtime overhead.
	usableBudget := budget * (percentDivisor - SlackPercent) / percentDivisor

	// Subtract base overhead.
	available := usableBudget - BaseOverhead
	if available <= 0 {
		return framework.CoordinatorConfig{}, ErrBudgetTooSmall
	}

	// Allocate proportionally.
	cacheAlloc := available * CacheAllocationPercent / percentDivisor
	workerAlloc := available * WorkerAllocationPercent / percentDivisor
	bufferAlloc := available * BufferAllocationPercent / percentDivisor

	cfg := deriveKnobs(cacheAlloc, workerAlloc, bufferAlloc)

	return cfg, nil
}

// deriveKnobs calculates individual configuration knobs from allocation budgets.
func deriveKnobs(cacheAlloc, workerAlloc, bufferAlloc int64) framework.CoordinatorConfig {
	// Workers: maximize within allocation, minimum 1, cap at optimal ratio of CPU cores.
	// Include native overhead (C/mmap) per worker in the cost calculation.
	maxWorkers := max(MinWorkers, runtime.NumCPU()*OptimalWorkerRatio/percentDivisor)
	workerCost := int64(RepoHandleSize + DefaultArenaSize + WorkerNativeOverhead)
	workers := max(MinWorkers, min(maxWorkers, int(workerAlloc/workerCost)))

	// Blob cache: 80% of cache allocation, capped to avoid dominating the budget.
	blobCacheSize := max(int64(MinBlobCacheSize), cacheAlloc*BlobCacheRatio/percentDivisor)
	blobCacheSize = min(blobCacheSize, MaxBlobCacheSize)

	// Diff cache: 20% of cache allocation, converted to entries, capped.
	diffCacheAlloc := cacheAlloc * DiffCacheRatio / percentDivisor
	diffCacheSize := max(MinDiffCacheSize, int(diffCacheAlloc/AvgDiffSize))
	diffCacheSize = min(diffCacheSize, MaxDiffCacheEntries)

	// Buffer size: based on allocation and workers.
	bufferSize := max(MinBufferSize, int(bufferAlloc/AvgCommitDataSize))

	// Use default arena size.
	arenaSize := DefaultArenaSize

	return framework.CoordinatorConfig{
		Workers:         workers,
		BufferSize:      bufferSize,
		CommitBatchSize: DefaultCommitBatchSize,
		BlobCacheSize:   blobCacheSize,
		DiffCacheSize:   diffCacheSize,
		BlobArenaSize:   arenaSize,
	}
}
