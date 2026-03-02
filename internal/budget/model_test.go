package budget

import (
	"testing"

	"github.com/stretchr/testify/assert"

	"github.com/Sumatoshi-tech/codefang/internal/framework"
)

// EstimateMemoryUsage calculates the estimated memory usage for a given configuration.
func EstimateMemoryUsage(cfg framework.CoordinatorConfig) int64 {
	workerMemory := int64(cfg.Workers) * (RepoHandleSize + int64(cfg.BlobArenaSize))
	nativeMemory := int64(cfg.Workers) * WorkerNativeOverhead
	cacheMemory := cfg.BlobCacheSize + int64(cfg.DiffCacheSize)*AvgDiffSize
	bufferMemory := int64(cfg.BufferSize) * AvgCommitDataSize

	return BaseOverhead + workerMemory + nativeMemory + cacheMemory + bufferMemory
}

func TestEstimateMemoryUsage_DefaultConfig(t *testing.T) {
	t.Parallel()

	cfg := framework.DefaultCoordinatorConfig()

	estimate := EstimateMemoryUsage(cfg)

	// Estimate should be positive and reasonable (at least base overhead).
	assert.Positive(t, estimate, "estimate should be positive")
	assert.GreaterOrEqual(t, estimate, int64(BaseOverhead), "estimate should include base overhead")
}

func TestEstimateMemoryUsage_MinimalConfig(t *testing.T) {
	t.Parallel()

	minimalCfg := framework.CoordinatorConfig{
		Workers:       1,
		BufferSize:    1,
		BlobCacheSize: 1 * MiB,
		DiffCacheSize: 100,
		BlobArenaSize: 1 * MiB,
	}
	defaultCfg := framework.DefaultCoordinatorConfig()

	minimalEstimate := EstimateMemoryUsage(minimalCfg)
	defaultEstimate := EstimateMemoryUsage(defaultCfg)

	// Minimal config should use less memory than default.
	assert.Less(t, minimalEstimate, defaultEstimate, "minimal config should use less memory")
	// But still include base overhead.
	assert.GreaterOrEqual(t, minimalEstimate, int64(BaseOverhead), "should include base overhead")
}

func TestEstimateMemoryUsage_Monotonic_Workers(t *testing.T) {
	t.Parallel()

	baseCfg := framework.CoordinatorConfig{
		Workers:       2,
		BufferSize:    4,
		BlobCacheSize: 100 * MiB,
		DiffCacheSize: 1000,
		BlobArenaSize: 4 * MiB,
	}
	moreCfg := baseCfg
	moreCfg.Workers = 4

	baseEstimate := EstimateMemoryUsage(baseCfg)
	moreEstimate := EstimateMemoryUsage(moreCfg)

	assert.Greater(t, moreEstimate, baseEstimate, "more workers should increase memory")
}

func TestEstimateMemoryUsage_Monotonic_BlobCache(t *testing.T) {
	t.Parallel()

	baseCfg := framework.CoordinatorConfig{
		Workers:       2,
		BufferSize:    4,
		BlobCacheSize: 100 * MiB,
		DiffCacheSize: 1000,
		BlobArenaSize: 4 * MiB,
	}
	moreCfg := baseCfg
	moreCfg.BlobCacheSize = 500 * MiB

	baseEstimate := EstimateMemoryUsage(baseCfg)
	moreEstimate := EstimateMemoryUsage(moreCfg)

	assert.Greater(t, moreEstimate, baseEstimate, "larger blob cache should increase memory")
}

func TestEstimateMemoryUsage_Monotonic_DiffCache(t *testing.T) {
	t.Parallel()

	baseCfg := framework.CoordinatorConfig{
		Workers:       2,
		BufferSize:    4,
		BlobCacheSize: 100 * MiB,
		DiffCacheSize: 1000,
		BlobArenaSize: 4 * MiB,
	}
	moreCfg := baseCfg
	moreCfg.DiffCacheSize = 10000

	baseEstimate := EstimateMemoryUsage(baseCfg)
	moreEstimate := EstimateMemoryUsage(moreCfg)

	assert.Greater(t, moreEstimate, baseEstimate, "larger diff cache should increase memory")
}
