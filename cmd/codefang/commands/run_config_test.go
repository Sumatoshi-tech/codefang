//go:build ignore

package commands

import (
	"context"
	"io"
	"os"
	"path/filepath"
	"testing"

	"github.com/stretchr/testify/require"
)

const (
	testConfigWorkers16 = 16
	testConfigWorkers32 = 32
)

func TestRunCommand_WithConfigFlag_LoadsFile(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	cfgPath := filepath.Join(dir, "custom.yaml")
	content := `analyzers:
  - history/devs
pipeline:
  workers: 16
`
	require.NoError(t, os.WriteFile(cfgPath, []byte(content), 0o600))

	var (
		seenIDs  []string
		seenOpts HistoryRunOptions
	)

	command := newRunCommandWithDeps(
		func(_ string, _ []string, _ string, _ bool, _ bool, _ io.Writer) error {
			return nil
		},
		func(_ context.Context, _ string, ids []string, _ string, _ bool, opts HistoryRunOptions, _ io.Writer) error {
			seenIDs = ids
			seenOpts = opts

			return nil
		},
		stubRunRegistry,
	)

	command.SetOut(io.Discard)
	command.SetErr(io.Discard)
	command.SetArgs([]string{"--config", cfgPath})

	err := command.Execute()
	require.NoError(t, err)
	require.Equal(t, []string{"history/devs"}, seenIDs)
	require.Equal(t, testConfigWorkers16, seenOpts.Workers)
}

func TestRunCommand_CLIFlagsOverrideConfig(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	cfgPath := filepath.Join(dir, "custom.yaml")
	content := `analyzers:
  - history/devs
pipeline:
  workers: 16
`
	require.NoError(t, os.WriteFile(cfgPath, []byte(content), 0o600))

	var (
		seenIDs  []string
		seenOpts HistoryRunOptions
	)

	command := newRunCommandWithDeps(
		func(_ string, ids []string, _ string, _ bool, _ bool, _ io.Writer) error {
			seenIDs = ids

			return nil
		},
		func(_ context.Context, _ string, ids []string, _ string, _ bool, opts HistoryRunOptions, _ io.Writer) error {
			seenIDs = ids
			seenOpts = opts

			return nil
		},
		stubRunRegistry,
	)

	command.SetOut(io.Discard)
	command.SetErr(io.Discard)
	command.SetArgs([]string{
		"--config", cfgPath,
		"-a", "static/complexity",
		"--workers", "32",
	})

	err := command.Execute()
	require.NoError(t, err)
	require.Equal(t, []string{"static/complexity"}, seenIDs)
	require.Equal(t, 0, seenOpts.Workers, "history should not be called for static-only run")
}

func TestRunCommand_NoConfig_DefaultBehavior(t *testing.T) {
	t.Parallel()

	var staticCalled bool

	command := newRunCommandWithDeps(
		func(_ string, ids []string, _ string, _ bool, _ bool, _ io.Writer) error {
			staticCalled = true

			require.Equal(t, []string{"static/complexity"}, ids)

			return nil
		},
		func(_ context.Context, _ string, _ []string, _ string, _ bool, _ HistoryRunOptions, _ io.Writer) error {
			return nil
		},
		stubRunRegistry,
	)

	command.SetOut(io.Discard)
	command.SetErr(io.Discard)
	command.SetArgs([]string{"-a", "static/complexity"})

	err := command.Execute()
	require.NoError(t, err)
	require.True(t, staticCalled)
}

func TestRunCommand_InvalidConfig_ReturnsError(t *testing.T) {
	t.Parallel()

	command := newRunCommandWithDeps(
		func(_ string, _ []string, _ string, _ bool, _ bool, _ io.Writer) error { return nil },
		func(_ context.Context, _ string, _ []string, _ string, _ bool, _ HistoryRunOptions, _ io.Writer) error {
			return nil
		},
		stubRunRegistry,
	)

	command.SetOut(io.Discard)
	command.SetErr(io.Discard)
	command.SetArgs([]string{"--config", "/nonexistent/path.yaml", "-a", "static/complexity"})

	err := command.Execute()
	require.Error(t, err)
	require.Contains(t, err.Error(), "load config")
}

func TestRunCommand_ConfigPipelineValues_ForwardedToHistory(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	cfgPath := filepath.Join(dir, "custom.yaml")
	content := `pipeline:
  workers: 32
  memory_budget: "8GB"
  gogc: 300
  ballast_size: "128MB"
  blob_cache_size: "2GB"
  diff_cache_size: 20000
  commit_batch_size: 500
`
	require.NoError(t, os.WriteFile(cfgPath, []byte(content), 0o600))

	var seenOpts HistoryRunOptions

	command := newRunCommandWithDeps(
		func(_ string, _ []string, _ string, _ bool, _ bool, _ io.Writer) error { return nil },
		func(_ context.Context, _ string, _ []string, _ string, _ bool, opts HistoryRunOptions, _ io.Writer) error {
			seenOpts = opts

			return nil
		},
		stubRunRegistry,
	)

	command.SetOut(io.Discard)
	command.SetErr(io.Discard)
	command.SetArgs([]string{"--config", cfgPath, "-a", "history/devs"})

	expectedDiffCache := 20000
	expectedBatchSize := 500
	expectedGOGC := 300

	err := command.Execute()
	require.NoError(t, err)
	require.Equal(t, testConfigWorkers32, seenOpts.Workers)
	require.Equal(t, "8GB", seenOpts.MemoryBudget)
	require.Equal(t, expectedGOGC, seenOpts.GCPercent)
	require.Equal(t, "128MB", seenOpts.BallastSize)
	require.Equal(t, "2GB", seenOpts.BlobCacheSize)
	require.Equal(t, expectedDiffCache, seenOpts.DiffCacheSize)
	require.Equal(t, expectedBatchSize, seenOpts.CommitBatchSize)
}

func TestRunCommand_ConfigCheckpoint_Forwarded(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	cfgPath := filepath.Join(dir, "custom.yaml")
	content := `checkpoint:
  dir: "/tmp/custom-ckpt"
  clear_prev: true
`
	require.NoError(t, os.WriteFile(cfgPath, []byte(content), 0o600))

	var seenOpts HistoryRunOptions

	command := newRunCommandWithDeps(
		func(_ string, _ []string, _ string, _ bool, _ bool, _ io.Writer) error { return nil },
		func(_ context.Context, _ string, _ []string, _ string, _ bool, opts HistoryRunOptions, _ io.Writer) error {
			seenOpts = opts

			return nil
		},
		stubRunRegistry,
	)

	command.SetOut(io.Discard)
	command.SetErr(io.Discard)
	command.SetArgs([]string{"--config", cfgPath, "-a", "history/devs"})

	err := command.Execute()
	require.NoError(t, err)
	require.Equal(t, "/tmp/custom-ckpt", seenOpts.CheckpointDir)
	require.True(t, seenOpts.ClearCheckpoint)
}

func TestRunCommand_ConfigAnalyzerConfig_PassedToHistory(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	cfgPath := filepath.Join(dir, "custom.yaml")
	content := `history:
  typos:
    max_distance: 6
`
	require.NoError(t, os.WriteFile(cfgPath, []byte(content), 0o600))

	var seenOpts HistoryRunOptions

	command := newRunCommandWithDeps(
		func(_ string, _ []string, _ string, _ bool, _ bool, _ io.Writer) error { return nil },
		func(_ context.Context, _ string, _ []string, _ string, _ bool, opts HistoryRunOptions, _ io.Writer) error {
			seenOpts = opts

			return nil
		},
		stubRunRegistry,
	)

	command.SetOut(io.Discard)
	command.SetErr(io.Discard)
	command.SetArgs([]string{"--config", cfgPath, "-a", "history/devs"})

	expectedMaxDistance := 6

	err := command.Execute()
	require.NoError(t, err)
	require.NotNil(t, seenOpts.AnalyzerConfig)
	require.Equal(t, expectedMaxDistance, seenOpts.AnalyzerConfig.History.Typos.MaxDistance)
}
