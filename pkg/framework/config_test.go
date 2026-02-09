package framework_test

import (
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/pkg/budget"
	"github.com/Sumatoshi-tech/codefang/pkg/framework"
)

func TestBuildConfigFromParams_Defaults(t *testing.T) {
	t.Parallel()

	config, memBudget, err := framework.BuildConfigFromParams(framework.ConfigParams{}, nil)
	require.NoError(t, err)

	defaultConfig := framework.DefaultCoordinatorConfig()
	assert.Equal(t, defaultConfig.Workers, config.Workers)
	assert.Equal(t, defaultConfig.BufferSize, config.BufferSize)
	assert.Equal(t, defaultConfig.CommitBatchSize, config.CommitBatchSize)
	assert.Equal(t, defaultConfig.BlobCacheSize, config.BlobCacheSize)
	assert.Equal(t, defaultConfig.DiffCacheSize, config.DiffCacheSize)
	assert.Equal(t, defaultConfig.BlobArenaSize, config.BlobArenaSize)
	assert.Zero(t, memBudget)
}

func TestBuildConfigFromParams_Workers(t *testing.T) {
	t.Parallel()

	config, _, err := framework.BuildConfigFromParams(framework.ConfigParams{Workers: 8}, nil)
	require.NoError(t, err)

	assert.Equal(t, 8, config.Workers)
}

func TestBuildConfigFromParams_BufferSize(t *testing.T) {
	t.Parallel()

	config, _, err := framework.BuildConfigFromParams(framework.ConfigParams{BufferSize: 32}, nil)
	require.NoError(t, err)

	assert.Equal(t, 32, config.BufferSize)
}

func TestBuildConfigFromParams_CommitBatchSize(t *testing.T) {
	t.Parallel()

	config, _, err := framework.BuildConfigFromParams(framework.ConfigParams{CommitBatchSize: 50}, nil)
	require.NoError(t, err)

	assert.Equal(t, 50, config.CommitBatchSize)
}

func TestBuildConfigFromParams_BlobCacheSize(t *testing.T) {
	t.Parallel()

	config, _, err := framework.BuildConfigFromParams(framework.ConfigParams{BlobCacheSize: "256MiB"}, nil)
	require.NoError(t, err)

	const expectedSize = 256 * 1024 * 1024
	assert.Equal(t, int64(expectedSize), config.BlobCacheSize)
}

func TestBuildConfigFromParams_BlobCacheSizeGigabytes(t *testing.T) {
	t.Parallel()

	config, _, err := framework.BuildConfigFromParams(framework.ConfigParams{BlobCacheSize: "2GiB"}, nil)
	require.NoError(t, err)

	const expectedSize = 2 * 1024 * 1024 * 1024
	assert.Equal(t, int64(expectedSize), config.BlobCacheSize)
}

func TestBuildConfigFromParams_DiffCacheSize(t *testing.T) {
	t.Parallel()

	config, _, err := framework.BuildConfigFromParams(framework.ConfigParams{DiffCacheSize: 5000}, nil)
	require.NoError(t, err)

	assert.Equal(t, 5000, config.DiffCacheSize)
}

func TestBuildConfigFromParams_BlobArenaSize(t *testing.T) {
	t.Parallel()

	config, _, err := framework.BuildConfigFromParams(framework.ConfigParams{BlobArenaSize: "8MiB"}, nil)
	require.NoError(t, err)

	const expectedSize = 8 * 1024 * 1024
	assert.Equal(t, expectedSize, config.BlobArenaSize)
}

func TestBuildConfigFromParams_InvalidBlobCacheSize(t *testing.T) {
	t.Parallel()

	_, _, err := framework.BuildConfigFromParams(framework.ConfigParams{BlobCacheSize: "invalid"}, nil)
	require.Error(t, err)
	assert.ErrorIs(t, err, framework.ErrInvalidSizeFormat)
}

func TestBuildConfigFromParams_InvalidBlobArenaSize(t *testing.T) {
	t.Parallel()

	_, _, err := framework.BuildConfigFromParams(framework.ConfigParams{BlobArenaSize: "notasize"}, nil)
	require.Error(t, err)
	assert.ErrorIs(t, err, framework.ErrInvalidSizeFormat)
}

func TestBuildConfigFromParams_AllParams(t *testing.T) {
	t.Parallel()

	config, _, err := framework.BuildConfigFromParams(framework.ConfigParams{
		Workers:         4,
		BufferSize:      16,
		CommitBatchSize: 25,
		BlobCacheSize:   "128MiB",
		DiffCacheSize:   2000,
		BlobArenaSize:   "4MiB",
	}, nil)
	require.NoError(t, err)

	assert.Equal(t, 4, config.Workers)
	assert.Equal(t, 16, config.BufferSize)
	assert.Equal(t, 25, config.CommitBatchSize)
	assert.Equal(t, int64(128*1024*1024), config.BlobCacheSize)
	assert.Equal(t, 2000, config.DiffCacheSize)
	assert.Equal(t, 4*1024*1024, config.BlobArenaSize)
}

func TestBuildConfigFromParams_MemoryBudget(t *testing.T) {
	t.Parallel()

	config, memBudget, err := framework.BuildConfigFromParams(
		framework.ConfigParams{MemoryBudget: "1GiB"},
		budget.SolveForBudget,
	)
	require.NoError(t, err)

	assert.Positive(t, config.Workers)
	assert.Positive(t, config.BufferSize)
	assert.Positive(t, config.BlobCacheSize)
	assert.Positive(t, config.DiffCacheSize)
	assert.Positive(t, memBudget)
}

func TestBuildConfigFromParams_MemoryBudget_TooSmall(t *testing.T) {
	t.Parallel()

	_, _, err := framework.BuildConfigFromParams(
		framework.ConfigParams{MemoryBudget: "64MiB"},
		budget.SolveForBudget,
	)
	require.Error(t, err)
	assert.ErrorIs(t, err, budget.ErrBudgetTooSmall)
}

func TestBuildConfigFromParams_MemoryBudget_InvalidFormat(t *testing.T) {
	t.Parallel()

	_, _, err := framework.BuildConfigFromParams(
		framework.ConfigParams{MemoryBudget: "notasize"},
		budget.SolveForBudget,
	)
	require.Error(t, err)
	assert.ErrorIs(t, err, framework.ErrInvalidSizeFormat)
}

func TestBuildConfigFromParams_GCPercent(t *testing.T) {
	t.Parallel()

	config, _, err := framework.BuildConfigFromParams(framework.ConfigParams{GCPercent: 200}, nil)
	require.NoError(t, err)

	assert.Equal(t, 200, config.GCPercent)
}

func TestBuildConfigFromParams_InvalidGCPercent(t *testing.T) {
	t.Parallel()

	_, _, err := framework.BuildConfigFromParams(framework.ConfigParams{GCPercent: -1}, nil)
	require.Error(t, err)
	assert.ErrorIs(t, err, framework.ErrInvalidGCPercent)
}

func TestBuildConfigFromParams_BallastSize(t *testing.T) {
	t.Parallel()

	config, _, err := framework.BuildConfigFromParams(framework.ConfigParams{BallastSize: "64MiB"}, nil)
	require.NoError(t, err)

	assert.Equal(t, int64(64*1024*1024), config.BallastSize)
}

func TestBuildConfigFromParams_BallastSize_Invalid(t *testing.T) {
	t.Parallel()

	_, _, err := framework.BuildConfigFromParams(framework.ConfigParams{BallastSize: "invalid"}, nil)
	require.Error(t, err)
	assert.ErrorIs(t, err, framework.ErrInvalidSizeFormat)
}

func TestBuildConfigFromParams_RuntimeFlagsWithBudget(t *testing.T) {
	t.Parallel()

	config, _, err := framework.BuildConfigFromParams(framework.ConfigParams{
		MemoryBudget: "1GiB",
		GCPercent:    220,
		BallastSize:  "32MiB",
	}, budget.SolveForBudget)
	require.NoError(t, err)

	assert.Equal(t, 220, config.GCPercent)
	assert.Equal(t, int64(32*1024*1024), config.BallastSize)
}
