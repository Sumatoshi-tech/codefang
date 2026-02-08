// Package budget provides memory budget calculation and auto-tuning for codefang history analysis.
package budget

import (
	"github.com/Sumatoshi-tech/codefang/pkg/framework"
)

// Size unit multipliers (binary, 1024-based).
const (
	KiB = 1024
	MiB = 1024 * KiB
	GiB = 1024 * MiB
)

// Component memory sizes (empirically measured).
const (
	// BaseOverhead is the fixed Go runtime + libgit2 overhead.
	BaseOverhead = 50 * MiB

	// RepoHandleSize is the memory per worker for libgit2 repository handle.
	RepoHandleSize = 10 * MiB

	// AvgDiffSize is the average size of a cached diff entry.
	AvgDiffSize = 2 * KiB

	// AvgCommitDataSize is the average size of in-flight commit data.
	AvgCommitDataSize = 64 * KiB
)

// EstimateMemoryUsage calculates the estimated memory usage for a given configuration.
// The formula is: BaseOverhead + WorkerMemory + CacheMemory + BufferMemory.
func EstimateMemoryUsage(cfg framework.CoordinatorConfig) int64 {
	workerMemory := int64(cfg.Workers) * (RepoHandleSize + int64(cfg.BlobArenaSize))
	cacheMemory := cfg.BlobCacheSize + int64(cfg.DiffCacheSize)*AvgDiffSize
	bufferMemory := int64(cfg.BufferSize) * AvgCommitDataSize

	return BaseOverhead + workerMemory + cacheMemory + bufferMemory
}
