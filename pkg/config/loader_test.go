package config_test

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/pkg/config"
)

const (
	testWorkers         = 8
	testDiffCacheSize   = 5000
	testCommitBatchSize = 200
	testGOGC            = 200
	testGranularity     = 15
	testSampling        = 15
	testHibThreshold    = 2000
	testGoroutines      = 8
	testMaxFileSize     = 2097152
	testMinCommentLen   = 30
	testSentimentGap    = 0.7
	testTyposMaxDist    = 3
)

func TestLoadConfig_NoFile_UsesDefaults(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	cfgPath := filepath.Join(dir, ".codefang.yaml")

	// Explicitly point to a non-existent file so viper reports "not found".
	cfg, err := config.LoadConfig(cfgPath)
	// File does not exist, but explicit path means viper returns an error.
	// Instead, test with an empty YAML file.
	_ = cfg
	_ = err

	emptyPath := filepath.Join(dir, "empty.yaml")
	require.NoError(t, os.WriteFile(emptyPath, []byte(""), 0o600))

	cfg, err = config.LoadConfig(emptyPath)
	require.NoError(t, err)
	require.NotNil(t, cfg)

	assert.Empty(t, cfg.Analyzers)
	assert.Equal(t, config.DefaultPipelineWorkers, cfg.Pipeline.Workers)
	assert.Equal(t, config.DefaultPipelineGOGC, cfg.Pipeline.GOGC)
	assert.Equal(t, config.DefaultPipelineBallastSize, cfg.Pipeline.BallastSize)
	assert.Equal(t, config.DefaultBurndownGranularity, cfg.History.Burndown.Granularity)
	assert.Equal(t, config.DefaultBurndownSampling, cfg.History.Burndown.Sampling)
	assert.Equal(t, config.DefaultBurndownTrackFiles, cfg.History.Burndown.TrackFiles)
	assert.Equal(t, config.DefaultBurndownHibernationThreshold, cfg.History.Burndown.HibernationThreshold)
	assert.Equal(t, config.DefaultDevsConsiderEmptyCommits, cfg.History.Devs.ConsiderEmptyCommits)
	assert.Equal(t, config.DefaultDevsAnonymize, cfg.History.Devs.Anonymize)
	assert.Equal(t, config.DefaultImportsGoroutines, cfg.History.Imports.Goroutines)
	assert.Equal(t, config.DefaultImportsMaxFileSize, cfg.History.Imports.MaxFileSize)
	assert.Equal(t, config.DefaultSentimentMinCommentLength, cfg.History.Sentiment.MinCommentLength)
	assert.InDelta(t, config.DefaultSentimentGap, cfg.History.Sentiment.Gap, 0.001)
	assert.Equal(t, config.DefaultShotnessDSLStruct, cfg.History.Shotness.DSLStruct)
	assert.Equal(t, config.DefaultShotnessDSLName, cfg.History.Shotness.DSLName)
	assert.Equal(t, config.DefaultTyposMaxDistance, cfg.History.Typos.MaxDistance)
	assert.Equal(t, config.DefaultCheckpointEnabled, cfg.Checkpoint.Enabled)
	assert.Equal(t, config.DefaultCheckpointResume, cfg.Checkpoint.Resume)
}

func TestLoadConfig_ValidFile_Unmarshals(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	cfgPath := filepath.Join(dir, ".codefang.yaml")
	content := `analyzers:
  - burndown
  - complexity
pipeline:
  workers: 8
  memory_budget: "4GB"
  blob_cache_size: "512MB"
  diff_cache_size: 5000
  commit_batch_size: 200
  gogc: 200
  ballast_size: "256MB"
history:
  burndown:
    granularity: 15
    sampling: 15
    track_files: true
    track_people: true
    hibernation_threshold: 2000
  devs:
    consider_empty_commits: true
    anonymize: true
  imports:
    goroutines: 8
    max_file_size: 2097152
  sentiment:
    min_comment_length: 30
    gap: 0.7
  shotness:
    dsl_struct: 'filter(.roles has "Class")'
    dsl_name: ".props.identifier"
  typos:
    max_distance: 3
checkpoint:
  enabled: false
  dir: "/tmp/ckpt"
  resume: false
  clear_prev: true
`
	require.NoError(t, os.WriteFile(cfgPath, []byte(content), 0o600))

	cfg, err := config.LoadConfig(cfgPath)
	require.NoError(t, err)
	require.NotNil(t, cfg)

	assert.Equal(t, []string{"burndown", "complexity"}, cfg.Analyzers)
	assert.Equal(t, testWorkers, cfg.Pipeline.Workers)
	assert.Equal(t, "4GB", cfg.Pipeline.MemoryBudget)
	assert.Equal(t, "512MB", cfg.Pipeline.BlobCacheSize)
	assert.Equal(t, testDiffCacheSize, cfg.Pipeline.DiffCacheSize)
	assert.Equal(t, testCommitBatchSize, cfg.Pipeline.CommitBatchSize)
	assert.Equal(t, testGOGC, cfg.Pipeline.GOGC)
	assert.Equal(t, "256MB", cfg.Pipeline.BallastSize)

	assert.Equal(t, testGranularity, cfg.History.Burndown.Granularity)
	assert.Equal(t, testSampling, cfg.History.Burndown.Sampling)
	assert.True(t, cfg.History.Burndown.TrackFiles)
	assert.True(t, cfg.History.Burndown.TrackPeople)
	assert.Equal(t, testHibThreshold, cfg.History.Burndown.HibernationThreshold)

	assert.True(t, cfg.History.Devs.ConsiderEmptyCommits)
	assert.True(t, cfg.History.Devs.Anonymize)

	assert.Equal(t, testGoroutines, cfg.History.Imports.Goroutines)
	assert.Equal(t, testMaxFileSize, cfg.History.Imports.MaxFileSize)

	assert.Equal(t, testMinCommentLen, cfg.History.Sentiment.MinCommentLength)
	assert.InDelta(t, testSentimentGap, cfg.History.Sentiment.Gap, 0.001)

	assert.Equal(t, `filter(.roles has "Class")`, cfg.History.Shotness.DSLStruct)
	assert.Equal(t, ".props.identifier", cfg.History.Shotness.DSLName)

	assert.Equal(t, testTyposMaxDist, cfg.History.Typos.MaxDistance)

	assert.False(t, cfg.Checkpoint.Enabled)
	assert.Equal(t, "/tmp/ckpt", cfg.Checkpoint.Dir)
	assert.False(t, cfg.Checkpoint.Resume)
	assert.True(t, cfg.Checkpoint.ClearPrev)
}

func TestLoadConfig_ExplicitPath_Overrides(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	cfgPath := filepath.Join(dir, "custom-config.yaml")
	content := `pipeline:
  workers: 16
`
	require.NoError(t, os.WriteFile(cfgPath, []byte(content), 0o600))

	cfg, err := config.LoadConfig(cfgPath)
	require.NoError(t, err)

	expectedWorkers := 16

	assert.Equal(t, expectedWorkers, cfg.Pipeline.Workers)
}

func TestLoadConfig_MalformedYAML_ReturnsError(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	cfgPath := filepath.Join(dir, "bad.yaml")
	content := `pipeline:
  workers: [invalid yaml
`
	require.NoError(t, os.WriteFile(cfgPath, []byte(content), 0o600))

	cfg, err := config.LoadConfig(cfgPath)
	require.Error(t, err)
	assert.Nil(t, cfg)
	assert.Contains(t, err.Error(), "read config")
}

func TestLoadConfig_UnknownKeys_NoError(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	cfgPath := filepath.Join(dir, ".codefang.yaml")
	content := `unknown_section:
  unknown_key: "value"
pipeline:
  workers: 4
`
	require.NoError(t, os.WriteFile(cfgPath, []byte(content), 0o600))

	cfg, err := config.LoadConfig(cfgPath)
	require.NoError(t, err)

	expectedWorkers := 4

	assert.Equal(t, expectedWorkers, cfg.Pipeline.Workers)
}

func TestLoadConfig_EmptyAnalyzers_NilSlice(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	cfgPath := filepath.Join(dir, ".codefang.yaml")
	content := `analyzers: []
`
	require.NoError(t, os.WriteFile(cfgPath, []byte(content), 0o600))

	cfg, err := config.LoadConfig(cfgPath)
	require.NoError(t, err)
	assert.Empty(t, cfg.Analyzers)
}

func TestLoadConfig_PartialConfig_MergesDefaults(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	cfgPath := filepath.Join(dir, ".codefang.yaml")
	content := `history:
  burndown:
    granularity: 60
`
	require.NoError(t, os.WriteFile(cfgPath, []byte(content), 0o600))

	cfg, err := config.LoadConfig(cfgPath)
	require.NoError(t, err)

	expectedGranularity := 60

	assert.Equal(t, expectedGranularity, cfg.History.Burndown.Granularity)
	assert.Equal(t, config.DefaultBurndownSampling, cfg.History.Burndown.Sampling)
	assert.Equal(t, config.DefaultPipelineWorkers, cfg.Pipeline.Workers)
	assert.Equal(t, config.DefaultTyposMaxDistance, cfg.History.Typos.MaxDistance)
}

func TestLoadConfig_EnvOverride_Pipeline(t *testing.T) {
	dir := t.TempDir()
	emptyPath := filepath.Join(dir, "empty.yaml")
	require.NoError(t, os.WriteFile(emptyPath, []byte(""), 0o600))

	t.Setenv("CODEFANG_PIPELINE_WORKERS", "32")

	cfg, err := config.LoadConfig(emptyPath)
	require.NoError(t, err)

	expectedWorkers := 32

	assert.Equal(t, expectedWorkers, cfg.Pipeline.Workers)
}

func TestLoadConfig_EnvOverride_NestedKey(t *testing.T) {
	dir := t.TempDir()
	emptyPath := filepath.Join(dir, "empty.yaml")
	require.NoError(t, os.WriteFile(emptyPath, []byte(""), 0o600))

	t.Setenv("CODEFANG_HISTORY_BURNDOWN_GRANULARITY", "60")

	cfg, err := config.LoadConfig(emptyPath)
	require.NoError(t, err)

	expectedGranularity := 60

	assert.Equal(t, expectedGranularity, cfg.History.Burndown.Granularity)
}

func TestLoadConfig_ExplicitPath_NotFound_ReturnsError(t *testing.T) {
	t.Parallel()

	cfg, err := config.LoadConfig("/nonexistent/path/config.yaml")
	require.Error(t, err)
	assert.Nil(t, cfg)
}
