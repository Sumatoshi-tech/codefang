// Package budget provides memory budget calculation and auto-tuning for codefang history analysis.
package budget

import "github.com/Sumatoshi-tech/codefang/pkg/units"

// Size unit multipliers re-exported from pkg/units for backwards compatibility.
const (
	KiB = units.KiB
	MiB = units.MiB
	GiB = units.GiB
)

// Component memory sizes (empirically measured).
const (
	// BaseOverhead is the fixed Go runtime + libgit2 overhead.
	// Includes shared mmap of pack files (~200 MB for large repos).
	BaseOverhead = 250 * MiB

	// RepoHandleSize is the Go-visible memory per worker for libgit2 repository handle.
	RepoHandleSize = 10 * MiB

	// WorkerNativeOverhead is the per-worker C/mmap overhead from libgit2.
	// Each worker opens the repo and mmaps pack index files; the OS faults in
	// pack data pages during object lookups. Empirically ~50-100 MB per worker
	// on large repos due to shared pack page cache pressure.
	WorkerNativeOverhead = 50 * MiB

	// AvgDiffSize is the average size of a cached diff entry.
	AvgDiffSize = 2 * KiB

	// AvgCommitDataSize is the average size of in-flight commit data.
	AvgCommitDataSize = 64 * KiB

	// MaxBlobCacheSize caps the blob cache to avoid dominating the budget.
	// Beyond 256 MB the hit rate improvement is marginal for most repositories.
	MaxBlobCacheSize = 256 * MiB

	// MaxDiffCacheEntries caps the diff cache. Beyond 20K entries the benefit
	// is marginal and memory cost grows linearly.
	MaxDiffCacheEntries = 20000

	// DefaultMwindowMappedLimit is libgit2's default mmap limit (8 GiB on 64-bit).
	// This allows pack file windows to consume enormous RSS on large repos.
	DefaultMwindowMappedLimit = 8 * GiB

	// DefaultLibgit2CacheSize is libgit2's default object cache (256 MiB).
	DefaultLibgit2CacheSize = 256 * MiB

	// NativeMemoryPercent is the fraction of the budget reserved for libgit2
	// native memory (mwindow + object cache + decompression buffers).
	// The rest is available to Go heap, caches, and buffers.
	NativeMemoryPercent = 25

	// MwindowCacheRatio controls how the native allocation is split:
	// 30% for mwindow (mmap'd pack data), 70% for object cache.
	// Lowered from 80 to reduce RSS from pack file mmap windows.
	// The larger object cache compensates by keeping decompressed objects
	// longer, reducing re-decompression overhead.
	MwindowCacheRatio = 30
)

// DefaultMallocArenaMax limits glibc malloc arenas to prevent RSS bloat.
// glibc defaults to 8*cores which retains freed memory across ~192 arenas
// on a 24-core machine, inflating RSS by 3-4x. A value of 2 reduces peak
// RSS by ~60% vs default, with minimal throughput impact when combined with
// malloc_trim(0) between chunks to reclaim freed arena memory.
const DefaultMallocArenaMax = 2

// NativeLimits holds libgit2 global memory limits derived from the budget.
type NativeLimits struct {
	MwindowMappedLimit int64
	CacheMaxSize       int64
	MallocArenaMax     int
}

// NativeLimitsForBudget computes libgit2 memory limits proportional to the
// memory budget. Returns zero values when no budget is set (use defaults).
func NativeLimitsForBudget(budget int64) NativeLimits {
	if budget <= 0 {
		return NativeLimits{}
	}

	nativeAlloc := budget * NativeMemoryPercent / percentDivisor
	mwindow := nativeAlloc * MwindowCacheRatio / percentDivisor
	cache := nativeAlloc - mwindow

	return NativeLimits{
		MwindowMappedLimit: mwindow,
		CacheMaxSize:       cache,
		MallocArenaMax:     DefaultMallocArenaMax,
	}
}
