package commands

import (
	"testing"

	"github.com/spf13/cobra"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/pkg/budget"
	"github.com/Sumatoshi-tech/codefang/pkg/framework"
)

func TestBuildCoordinatorConfig_Defaults(t *testing.T) {
	t.Parallel()

	cmd := &cobra.Command{}
	registerResourceKnobFlags(cmd)

	config, err := buildCoordinatorConfig(cmd)
	require.NoError(t, err)

	// Should return default config when no flags are set.
	defaultConfig := framework.DefaultCoordinatorConfig()
	assert.Equal(t, defaultConfig.Workers, config.Workers)
	assert.Equal(t, defaultConfig.BufferSize, config.BufferSize)
	assert.Equal(t, defaultConfig.CommitBatchSize, config.CommitBatchSize)
	assert.Equal(t, defaultConfig.BlobCacheSize, config.BlobCacheSize)
	assert.Equal(t, defaultConfig.DiffCacheSize, config.DiffCacheSize)
	assert.Equal(t, defaultConfig.BlobArenaSize, config.BlobArenaSize)
}

func TestBuildCoordinatorConfig_Workers(t *testing.T) {
	t.Parallel()

	cmd := &cobra.Command{}
	registerResourceKnobFlags(cmd)

	err := cmd.Flags().Set("workers", "8")
	require.NoError(t, err)

	config, err := buildCoordinatorConfig(cmd)
	require.NoError(t, err)

	assert.Equal(t, 8, config.Workers)
}

func TestBuildCoordinatorConfig_BufferSize(t *testing.T) {
	t.Parallel()

	cmd := &cobra.Command{}
	registerResourceKnobFlags(cmd)

	err := cmd.Flags().Set("buffer-size", "32")
	require.NoError(t, err)

	config, err := buildCoordinatorConfig(cmd)
	require.NoError(t, err)

	assert.Equal(t, 32, config.BufferSize)
}

func TestBuildCoordinatorConfig_CommitBatchSize(t *testing.T) {
	t.Parallel()

	cmd := &cobra.Command{}
	registerResourceKnobFlags(cmd)

	err := cmd.Flags().Set("commit-batch-size", "50")
	require.NoError(t, err)

	config, err := buildCoordinatorConfig(cmd)
	require.NoError(t, err)

	assert.Equal(t, 50, config.CommitBatchSize)
}

func TestBuildCoordinatorConfig_BlobCacheSize(t *testing.T) {
	t.Parallel()

	cmd := &cobra.Command{}
	registerResourceKnobFlags(cmd)

	// Use MiB for binary (1024-based) units.
	err := cmd.Flags().Set("blob-cache-size", "256MiB")
	require.NoError(t, err)

	config, err := buildCoordinatorConfig(cmd)
	require.NoError(t, err)

	const expectedSize = 256 * 1024 * 1024
	assert.Equal(t, int64(expectedSize), config.BlobCacheSize)
}

func TestBuildCoordinatorConfig_BlobCacheSizeGigabytes(t *testing.T) {
	t.Parallel()

	cmd := &cobra.Command{}
	registerResourceKnobFlags(cmd)

	// Use GiB for binary (1024-based) units.
	err := cmd.Flags().Set("blob-cache-size", "2GiB")
	require.NoError(t, err)

	config, err := buildCoordinatorConfig(cmd)
	require.NoError(t, err)

	const expectedSize = 2 * 1024 * 1024 * 1024
	assert.Equal(t, int64(expectedSize), config.BlobCacheSize)
}

func TestBuildCoordinatorConfig_DiffCacheSize(t *testing.T) {
	t.Parallel()

	cmd := &cobra.Command{}
	registerResourceKnobFlags(cmd)

	err := cmd.Flags().Set("diff-cache-size", "5000")
	require.NoError(t, err)

	config, err := buildCoordinatorConfig(cmd)
	require.NoError(t, err)

	assert.Equal(t, 5000, config.DiffCacheSize)
}

func TestBuildCoordinatorConfig_BlobArenaSize(t *testing.T) {
	t.Parallel()

	cmd := &cobra.Command{}
	registerResourceKnobFlags(cmd)

	// Use MiB for binary (1024-based) units.
	err := cmd.Flags().Set("blob-arena-size", "8MiB")
	require.NoError(t, err)

	config, err := buildCoordinatorConfig(cmd)
	require.NoError(t, err)

	const expectedSize = 8 * 1024 * 1024
	assert.Equal(t, expectedSize, config.BlobArenaSize)
}

func TestBuildCoordinatorConfig_InvalidBlobCacheSize(t *testing.T) {
	t.Parallel()

	cmd := &cobra.Command{}
	registerResourceKnobFlags(cmd)

	err := cmd.Flags().Set("blob-cache-size", "invalid")
	require.NoError(t, err)

	_, err = buildCoordinatorConfig(cmd)
	require.Error(t, err)
	assert.ErrorIs(t, err, ErrInvalidSizeFormat)
}

func TestBuildCoordinatorConfig_InvalidBlobArenaSize(t *testing.T) {
	t.Parallel()

	cmd := &cobra.Command{}
	registerResourceKnobFlags(cmd)

	err := cmd.Flags().Set("blob-arena-size", "notasize")
	require.NoError(t, err)

	_, err = buildCoordinatorConfig(cmd)
	require.Error(t, err)
	assert.ErrorIs(t, err, ErrInvalidSizeFormat)
}

func TestBuildCoordinatorConfig_AllFlags(t *testing.T) {
	t.Parallel()

	cmd := &cobra.Command{}
	registerResourceKnobFlags(cmd)

	// Use MiB/GiB for binary (1024-based) units.
	require.NoError(t, cmd.Flags().Set("workers", "4"))
	require.NoError(t, cmd.Flags().Set("buffer-size", "16"))
	require.NoError(t, cmd.Flags().Set("commit-batch-size", "25"))
	require.NoError(t, cmd.Flags().Set("blob-cache-size", "128MiB"))
	require.NoError(t, cmd.Flags().Set("diff-cache-size", "2000"))
	require.NoError(t, cmd.Flags().Set("blob-arena-size", "4MiB"))

	config, err := buildCoordinatorConfig(cmd)
	require.NoError(t, err)

	assert.Equal(t, 4, config.Workers)
	assert.Equal(t, 16, config.BufferSize)
	assert.Equal(t, 25, config.CommitBatchSize)
	assert.Equal(t, int64(128*1024*1024), config.BlobCacheSize)
	assert.Equal(t, 2000, config.DiffCacheSize)
	assert.Equal(t, 4*1024*1024, config.BlobArenaSize)
}

// registerResourceKnobFlags registers the resource knob flags on a command for testing.
func registerResourceKnobFlags(cmd *cobra.Command) {
	cmd.Flags().Int("workers", 0, "Number of parallel workers (0 = use CPU count)")
	cmd.Flags().Int("buffer-size", 0, "Size of internal pipeline channels (0 = workers×2)")
	cmd.Flags().Int("commit-batch-size", 0, "Commits per processing batch (0 = default 100)")
	cmd.Flags().String("blob-cache-size", "", "Max blob cache size (e.g., '256MB', '1GB'; empty = default 1GB)")
	cmd.Flags().Int("diff-cache-size", 0, "Max diff cache entries (0 = default 10000)")
	cmd.Flags().String("blob-arena-size", "", "Memory arena size for blob loading (e.g., '4MB'; empty = default 4MB)")
	cmd.Flags().String("memory-budget", "", "Memory budget for auto-tuning (e.g., '512MB', '2GB')")
}

func TestBuildCoordinatorConfig_MemoryBudget(t *testing.T) {
	t.Parallel()

	cmd := &cobra.Command{}
	registerResourceKnobFlags(cmd)

	err := cmd.Flags().Set("memory-budget", "1GiB")
	require.NoError(t, err)

	config, err := buildCoordinatorConfig(cmd)
	require.NoError(t, err)

	// Memory budget should produce a valid config.
	assert.Positive(t, config.Workers)
	assert.Positive(t, config.BufferSize)
	assert.Positive(t, config.BlobCacheSize)
	assert.Positive(t, config.DiffCacheSize)
}

func TestBuildCoordinatorConfig_MemoryBudget_TooSmall(t *testing.T) {
	t.Parallel()

	cmd := &cobra.Command{}
	registerResourceKnobFlags(cmd)

	err := cmd.Flags().Set("memory-budget", "64MiB")
	require.NoError(t, err)

	_, err = buildCoordinatorConfig(cmd)
	require.Error(t, err)
	assert.ErrorIs(t, err, budget.ErrBudgetTooSmall)
}

func TestBuildCoordinatorConfig_MemoryBudget_InvalidFormat(t *testing.T) {
	t.Parallel()

	cmd := &cobra.Command{}
	registerResourceKnobFlags(cmd)

	err := cmd.Flags().Set("memory-budget", "notasize")
	require.NoError(t, err)

	_, err = buildCoordinatorConfig(cmd)
	require.Error(t, err)
	assert.ErrorIs(t, err, ErrInvalidSizeFormat)
}
