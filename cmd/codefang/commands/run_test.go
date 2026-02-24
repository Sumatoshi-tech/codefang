package commands

import (
	"bytes"
	"context"
	"encoding/json"
	"io"
	"os"
	"path/filepath"
	"reflect"
	"slices"
	"testing"
	"time"

	"github.com/stretchr/testify/require"
	sdktrace "go.opentelemetry.io/otel/sdk/trace"
	"go.opentelemetry.io/otel/sdk/trace/tracetest"
	"gopkg.in/yaml.v3"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/common/renderer"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/common/reportutil"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
	"github.com/Sumatoshi-tech/codefang/pkg/observability"
	"github.com/Sumatoshi-tech/codefang/pkg/pipeline"
	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

func TestMain(m *testing.M) {
	renderer.RegisterPlotRenderer()
	os.Exit(m.Run())
}

type stubStaticRunAnalyzer struct {
	descriptor analyze.Descriptor
}

func (s *stubStaticRunAnalyzer) Name() string                                             { return s.descriptor.ID }
func (s *stubStaticRunAnalyzer) Flag() string                                             { return s.descriptor.ID }
func (s *stubStaticRunAnalyzer) Description() string                                      { return s.descriptor.Description }
func (s *stubStaticRunAnalyzer) Descriptor() analyze.Descriptor                           { return s.descriptor }
func (s *stubStaticRunAnalyzer) ListConfigurationOptions() []pipeline.ConfigurationOption { return nil }
func (s *stubStaticRunAnalyzer) Configure(_ map[string]any) error                         { return nil }
func (s *stubStaticRunAnalyzer) Analyze(_ *node.Node) (analyze.Report, error) {
	return analyze.Report{}, nil
}
func (s *stubStaticRunAnalyzer) Thresholds() analyze.Thresholds                   { return nil }
func (s *stubStaticRunAnalyzer) CreateAggregator() analyze.ResultAggregator       { return nil }
func (s *stubStaticRunAnalyzer) FormatReport(_ analyze.Report, _ io.Writer) error { return nil }
func (s *stubStaticRunAnalyzer) FormatReportJSON(_ analyze.Report, _ io.Writer) error {
	return nil
}
func (s *stubStaticRunAnalyzer) FormatReportYAML(_ analyze.Report, _ io.Writer) error {
	return nil
}
func (s *stubStaticRunAnalyzer) FormatReportPlot(_ analyze.Report, _ io.Writer) error {
	return nil
}
func (s *stubStaticRunAnalyzer) FormatReportBinary(_ analyze.Report, _ io.Writer) error {
	return nil
}

type stubHistoryRunAnalyzer struct {
	descriptor analyze.Descriptor
}

func (s *stubHistoryRunAnalyzer) Name() string                   { return s.descriptor.ID }
func (s *stubHistoryRunAnalyzer) Flag() string                   { return s.descriptor.ID }
func (s *stubHistoryRunAnalyzer) Description() string            { return s.descriptor.Description }
func (s *stubHistoryRunAnalyzer) Descriptor() analyze.Descriptor { return s.descriptor }
func (s *stubHistoryRunAnalyzer) ListConfigurationOptions() []pipeline.ConfigurationOption {
	return nil
}
func (s *stubHistoryRunAnalyzer) Configure(_ map[string]any) error      { return nil }
func (s *stubHistoryRunAnalyzer) Initialize(_ *gitlib.Repository) error { return nil }
func (s *stubHistoryRunAnalyzer) Consume(_ context.Context, _ *analyze.Context) (analyze.TC, error) {
	return analyze.TC{}, nil
}
func (s *stubHistoryRunAnalyzer) Fork(_ int) []analyze.HistoryAnalyzer { return nil }
func (s *stubHistoryRunAnalyzer) Merge(_ []analyze.HistoryAnalyzer)    {}
func (s *stubHistoryRunAnalyzer) WorkingStateSize() int64              { return 0 }
func (s *stubHistoryRunAnalyzer) AvgTCSize() int64                     { return 0 }
func (s *stubHistoryRunAnalyzer) NewAggregator(_ analyze.AggregatorOptions) analyze.Aggregator {
	return nil
}
func (s *stubHistoryRunAnalyzer) SerializeTICKs(_ []analyze.TICK, _ string, _ io.Writer) error {
	return analyze.ErrNotImplemented
}
func (s *stubHistoryRunAnalyzer) ReportFromTICKs(_ context.Context, _ []analyze.TICK) (analyze.Report, error) {
	return nil, analyze.ErrNotImplemented
}
func (s *stubHistoryRunAnalyzer) Serialize(_ analyze.Report, _ string, _ io.Writer) error {
	return nil
}

func TestRunCommand_DispatchesBothModes(t *testing.T) {
	t.Parallel()

	var (
		staticCalled  bool
		historyCalled bool
		staticFormat  string
		historyFormat string
	)

	command := newRunCommandWithDeps(
		func(_ string, ids []string, format string, _ bool, _ bool, writer io.Writer) error {
			staticCalled = true
			staticFormat = format

			require.Equal(t, []string{"static/complexity"}, ids)

			return reportutil.EncodeBinaryEnvelope(analyze.Report{"source": "static"}, writer)
		},
		func(_ context.Context, _ string, ids []string, format string, _ bool, _ HistoryRunOptions, writer io.Writer) error {
			historyCalled = true
			historyFormat = format

			require.Equal(t, []string{"history/devs"}, ids)

			return reportutil.EncodeBinaryEnvelope(analyze.Report{"source": "history"}, writer)
		},
		stubRunRegistry,
		noopObservabilityInit,
	)

	command.SetArgs([]string{"-a", "static/complexity,history/devs", "--path", ".", "--format", "bin"})
	err := command.Execute()
	require.NoError(t, err)
	require.True(t, staticCalled)
	require.True(t, historyCalled)
	require.Equal(t, analyze.FormatBinary, staticFormat)
	require.Equal(t, analyze.FormatBinary, historyFormat)
}

func TestRunCommand_StaticOnly(t *testing.T) {
	t.Parallel()

	var historyCalled bool

	command := newRunCommandWithDeps(
		func(_ string, ids []string, _ string, _ bool, _ bool, _ io.Writer) error {
			require.Equal(t, []string{"static/complexity"}, ids)

			return nil
		},
		func(_ context.Context, _ string, _ []string, _ string, _ bool, _ HistoryRunOptions, _ io.Writer) error {
			historyCalled = true

			return nil
		},
		stubRunRegistry,
		noopObservabilityInit,
	)

	command.SetArgs([]string{"-a", "static/complexity"})
	err := command.Execute()
	require.NoError(t, err)
	require.False(t, historyCalled)
}

func TestRunCommand_ProgressOutput_DefaultEnabled(t *testing.T) {
	t.Parallel()

	command := newRunCommandWithDeps(
		func(_ string, ids []string, format string, _ bool, _ bool, _ io.Writer) error {
			require.Equal(t, []string{"static/complexity"}, ids)
			require.Equal(t, analyze.FormatJSON, format)

			return nil
		},
		func(_ context.Context, _ string, _ []string, _ string, _ bool, _ HistoryRunOptions, _ io.Writer) error {
			t.Fatal("history executor should not be called")

			return nil
		},
		stubRunRegistry,
		noopObservabilityInit,
	)

	var errOut bytes.Buffer
	command.SetErr(&errOut)
	command.SetOut(io.Discard)
	command.SetArgs([]string{"-a", "static/complexity", "--format", "json"})
	err := command.Execute()
	require.NoError(t, err)
	require.Contains(t, errOut.String(), "progress: starting run")
	require.Contains(t, errOut.String(), "progress: static phase started")
	require.Contains(t, errOut.String(), "progress: run completed")
}

func TestRunCommand_ProgressOutput_Silent(t *testing.T) {
	t.Parallel()

	var historySilent bool

	command := newRunCommandWithDeps(
		func(_ string, _ []string, _ string, _ bool, _ bool, _ io.Writer) error {
			t.Fatal("static executor should not be called")

			return nil
		},
		func(_ context.Context, _ string, ids []string, format string, silent bool, _ HistoryRunOptions, _ io.Writer) error {
			historySilent = silent

			require.Equal(t, []string{"history/devs"}, ids)
			require.Equal(t, analyze.FormatJSON, format)

			return nil
		},
		stubRunRegistry,
		noopObservabilityInit,
	)

	var errOut bytes.Buffer
	command.SetErr(&errOut)
	command.SetOut(io.Discard)
	command.SetArgs([]string{"-a", "history/devs", "--format", "json", "--silent"})
	err := command.Execute()
	require.NoError(t, err)
	require.True(t, historySilent)
	require.Empty(t, errOut.String())
}

func TestRunCommand_ForwardsHistoryRuntimeOptions(t *testing.T) {
	t.Parallel()

	var seenOptions HistoryRunOptions

	command := newRunCommandWithDeps(
		func(_ string, _ []string, _ string, _ bool, _ bool, _ io.Writer) error {
			t.Fatal("static executor should not be called")

			return nil
		},
		func(_ context.Context, _ string, _ []string, _ string, _ bool, opts HistoryRunOptions, _ io.Writer) error {
			seenOptions = opts

			return nil
		},
		stubRunRegistry,
		noopObservabilityInit,
	)

	command.SetArgs([]string{
		"-a", "history/devs",
		"--gogc", "220",
		"--ballast-size", "64MiB",
	})

	err := command.Execute()
	require.NoError(t, err)
	require.Equal(t, 220, seenOptions.GCPercent)
	require.Equal(t, "64MiB", seenOptions.BallastSize)
}

func TestRunCommand_ForwardsCommitSelectionFlags(t *testing.T) {
	t.Parallel()

	var seenOptions HistoryRunOptions

	command := newRunCommandWithDeps(
		func(_ string, _ []string, _ string, _ bool, _ bool, _ io.Writer) error {
			return nil
		},
		func(_ context.Context, _ string, _ []string, _ string, _ bool, opts HistoryRunOptions, _ io.Writer) error {
			seenOptions = opts

			return nil
		},
		stubRunRegistry,
		noopObservabilityInit,
	)

	command.SetArgs([]string{
		"-a", "history/devs",
		"--limit", "1000",
		"--first-parent",
		"--head",
		"--since", "2024-01-01",
	})

	err := command.Execute()
	require.NoError(t, err)
	require.Equal(t, 1000, seenOptions.Limit)
	require.True(t, seenOptions.FirstParent)
	require.True(t, seenOptions.Head)
	require.Equal(t, "2024-01-01", seenOptions.Since)
}

func TestRunCommand_ForwardsProfilingFlags(t *testing.T) {
	t.Parallel()

	var seenOptions HistoryRunOptions

	command := newRunCommandWithDeps(
		func(_ string, _ []string, _ string, _ bool, _ bool, _ io.Writer) error {
			return nil
		},
		func(_ context.Context, _ string, _ []string, _ string, _ bool, opts HistoryRunOptions, _ io.Writer) error {
			seenOptions = opts

			return nil
		},
		stubRunRegistry,
		noopObservabilityInit,
	)

	command.SetArgs([]string{
		"-a", "history/devs",
		"--cpuprofile", "/tmp/cpu.prof",
		"--heapprofile", "/tmp/heap.prof",
	})

	err := command.Execute()
	require.NoError(t, err)
	require.Equal(t, "/tmp/cpu.prof", seenOptions.CPUProfile)
	require.Equal(t, "/tmp/heap.prof", seenOptions.HeapProfile)
}

func TestRunCommand_ForwardsResourceTuningFlags(t *testing.T) {
	t.Parallel()

	var seenOptions HistoryRunOptions

	command := newRunCommandWithDeps(
		func(_ string, _ []string, _ string, _ bool, _ bool, _ io.Writer) error {
			return nil
		},
		func(_ context.Context, _ string, _ []string, _ string, _ bool, opts HistoryRunOptions, _ io.Writer) error {
			seenOptions = opts

			return nil
		},
		stubRunRegistry,
		noopObservabilityInit,
	)

	command.SetArgs([]string{
		"-a", "history/devs",
		"--workers", "4",
		"--buffer-size", "8",
		"--commit-batch-size", "200",
		"--blob-cache-size", "512MB",
		"--diff-cache-size", "5000",
		"--blob-arena-size", "8MB",
		"--memory-budget", "2GB",
	})

	err := command.Execute()
	require.NoError(t, err)
	require.Equal(t, 4, seenOptions.Workers)
	require.Equal(t, 8, seenOptions.BufferSize)
	require.Equal(t, 200, seenOptions.CommitBatchSize)
	require.Equal(t, "512MB", seenOptions.BlobCacheSize)
	require.Equal(t, 5000, seenOptions.DiffCacheSize)
	require.Equal(t, "8MB", seenOptions.BlobArenaSize)
	require.Equal(t, "2GB", seenOptions.MemoryBudget)
}

func TestRunCommand_ForwardsCheckpointFlags(t *testing.T) {
	t.Parallel()

	var seenOptions HistoryRunOptions

	command := newRunCommandWithDeps(
		func(_ string, _ []string, _ string, _ bool, _ bool, _ io.Writer) error {
			return nil
		},
		func(_ context.Context, _ string, _ []string, _ string, _ bool, opts HistoryRunOptions, _ io.Writer) error {
			seenOptions = opts

			return nil
		},
		stubRunRegistry,
		noopObservabilityInit,
	)

	command.SetArgs([]string{
		"-a", "history/devs",
		"--checkpoint=false",
		"--resume=false",
		"--clear-checkpoint",
		"--checkpoint-dir", "/tmp/ckpt",
	})

	err := command.Execute()
	require.NoError(t, err)
	require.NotNil(t, seenOptions.Checkpoint)
	require.False(t, *seenOptions.Checkpoint)
	require.NotNil(t, seenOptions.Resume)
	require.False(t, *seenOptions.Resume)
	require.True(t, seenOptions.ClearCheckpoint)
	require.Equal(t, "/tmp/ckpt", seenOptions.CheckpointDir)
}

func TestRunCommand_CheckpointDefaultsPreserved(t *testing.T) {
	t.Parallel()

	var seenOptions HistoryRunOptions

	command := newRunCommandWithDeps(
		func(_ string, _ []string, _ string, _ bool, _ bool, _ io.Writer) error {
			return nil
		},
		func(_ context.Context, _ string, _ []string, _ string, _ bool, opts HistoryRunOptions, _ io.Writer) error {
			seenOptions = opts

			return nil
		},
		stubRunRegistry,
		noopObservabilityInit,
	)

	command.SetArgs([]string{"-a", "history/devs"})

	err := command.Execute()
	require.NoError(t, err)
	require.Nil(t, seenOptions.Checkpoint, "checkpoint should be nil when not explicitly set")
	require.Nil(t, seenOptions.Resume, "resume should be nil when not explicitly set")
}

func TestRunCommand_ProgressOutput_Quiet(t *testing.T) {
	t.Parallel()

	command := newRunCommandWithDeps(
		func(_ string, ids []string, format string, _ bool, _ bool, _ io.Writer) error {
			require.Equal(t, []string{"static/complexity"}, ids)
			require.Equal(t, analyze.FormatJSON, format)

			return nil
		},
		func(_ context.Context, _ string, _ []string, _ string, _ bool, _ HistoryRunOptions, _ io.Writer) error {
			t.Fatal("history executor should not be called")

			return nil
		},
		stubRunRegistry,
		noopObservabilityInit,
	)

	command.PersistentFlags().BoolP("quiet", "q", false, "suppress output")

	var errOut bytes.Buffer
	command.SetErr(&errOut)
	command.SetOut(io.Discard)
	command.SetArgs([]string{"-a", "static/complexity", "--format", "json", "-q"})
	err := command.Execute()
	require.NoError(t, err)
	require.Empty(t, errOut.String())
}

func TestRunCommand_UnknownAnalyzer(t *testing.T) {
	t.Parallel()

	command := newRunCommandWithDeps(
		func(_ string, _ []string, _ string, _ bool, _ bool, _ io.Writer) error { return nil },
		func(_ context.Context, _ string, _ []string, _ string, _ bool, _ HistoryRunOptions, _ io.Writer) error {
			return nil
		},
		stubRunRegistry,
		noopObservabilityInit,
	)

	command.SetArgs([]string{"-a", "unknown/id"})
	err := command.Execute()
	require.ErrorIs(t, err, analyze.ErrUnknownAnalyzerID)
}

func TestRunCommand_GlobStaticAnalyzers(t *testing.T) {
	t.Parallel()

	var historyCalled bool

	command := newRunCommandWithDeps(
		func(_ string, ids []string, format string, _ bool, _ bool, _ io.Writer) error {
			require.Equal(t, []string{"static/complexity"}, ids)
			require.Equal(t, analyze.FormatJSON, format)

			return nil
		},
		func(_ context.Context, _ string, _ []string, _ string, _ bool, _ HistoryRunOptions, _ io.Writer) error {
			historyCalled = true

			return nil
		},
		stubRunRegistry,
		noopObservabilityInit,
	)

	command.SetArgs([]string{"--analyzers", "static/*", "--format", "json"})
	err := command.Execute()
	require.NoError(t, err)
	require.False(t, historyCalled)
}

func TestRunCommand_GlobAllAnalyzers(t *testing.T) {
	t.Parallel()

	var (
		staticCalled  bool
		historyCalled bool
		staticFormat  string
		historyFormat string
	)

	command := newRunCommandWithDeps(
		func(_ string, ids []string, format string, _ bool, _ bool, writer io.Writer) error {
			staticCalled = true
			staticFormat = format

			require.Equal(t, []string{"static/complexity"}, ids)

			return reportutil.EncodeBinaryEnvelope(analyze.Report{"source": "static"}, writer)
		},
		func(_ context.Context, _ string, ids []string, format string, _ bool, _ HistoryRunOptions, writer io.Writer) error {
			historyCalled = true
			historyFormat = format

			require.Equal(t, []string{"history/devs"}, ids)

			return reportutil.EncodeBinaryEnvelope(analyze.Report{"source": "history"}, writer)
		},
		stubRunRegistry,
		noopObservabilityInit,
	)

	command.SetArgs([]string{"--analyzers", "*", "--format", "json"})
	err := command.Execute()
	require.NoError(t, err)
	require.True(t, staticCalled)
	require.True(t, historyCalled)
	require.Equal(t, analyze.FormatBinary, staticFormat)
	require.Equal(t, analyze.FormatBinary, historyFormat)
}

func TestRunCommand_GlobUnknownPattern(t *testing.T) {
	t.Parallel()

	command := newRunCommandWithDeps(
		func(_ string, _ []string, _ string, _ bool, _ bool, _ io.Writer) error { return nil },
		func(_ context.Context, _ string, _ []string, _ string, _ bool, _ HistoryRunOptions, _ io.Writer) error {
			return nil
		},
		stubRunRegistry,
		noopObservabilityInit,
	)

	command.SetArgs([]string{"--analyzers", "static/unknown*"})
	err := command.Execute()
	require.ErrorIs(t, err, analyze.ErrUnknownAnalyzerID)
}

func TestRunCommand_GlobInvalidPattern(t *testing.T) {
	t.Parallel()

	command := newRunCommandWithDeps(
		func(_ string, _ []string, _ string, _ bool, _ bool, _ io.Writer) error { return nil },
		func(_ context.Context, _ string, _ []string, _ string, _ bool, _ HistoryRunOptions, _ io.Writer) error {
			return nil
		},
		stubRunRegistry,
		noopObservabilityInit,
	)

	command.SetArgs([]string{"--analyzers", "[bad"})
	err := command.Execute()
	require.ErrorIs(t, err, analyze.ErrInvalidAnalyzerGlob)
}

func TestResolveFormats_MixedSupportsUniversalFormats(t *testing.T) {
	t.Parallel()

	testCases := []struct {
		name        string
		format      string
		wantStatic  string
		wantHistory string
	}{
		{name: "json", format: "json", wantStatic: analyze.FormatJSON, wantHistory: analyze.FormatJSON},
		{name: "yaml", format: analyze.FormatYAML, wantStatic: analyze.FormatYAML, wantHistory: analyze.FormatYAML},
		{name: "plot", format: analyze.FormatPlot, wantStatic: analyze.FormatPlot, wantHistory: analyze.FormatPlot},
		{name: "bin", format: "bin", wantStatic: analyze.FormatBinary, wantHistory: analyze.FormatBinary},
	}

	for _, testCase := range testCases {
		t.Run(testCase.name, func(t *testing.T) {
			t.Parallel()

			staticFormat, historyFormat, err := analyze.ResolveFormats(testCase.format, true, true)
			require.NoError(t, err)
			require.Equal(t, testCase.wantStatic, staticFormat)
			require.Equal(t, testCase.wantHistory, historyFormat)
		})
	}
}

func TestResolveFormats_MixedAcceptsText(t *testing.T) {
	t.Parallel()

	staticFmt, historyFmt, err := analyze.ResolveFormats(analyze.FormatText, true, true)
	require.NoError(t, err)
	require.Equal(t, analyze.FormatText, staticFmt)
	require.Equal(t, analyze.FormatText, historyFmt)
}

func TestResolveFormats_MixedRejectsInvalid(t *testing.T) {
	t.Parallel()

	_, _, err := analyze.ResolveFormats("html", true, true)
	require.ErrorIs(t, err, analyze.ErrInvalidMixedFormat)
}

func TestResolveInputFormat(t *testing.T) {
	t.Parallel()

	testCases := []struct {
		name        string
		inputPath   string
		inputFormat string
		want        string
	}{
		{name: "auto json", inputPath: "report.json", inputFormat: analyze.InputFormatAuto, want: analyze.FormatJSON},
		{name: "auto bin", inputPath: "report.bin", inputFormat: analyze.InputFormatAuto, want: analyze.FormatBinary},
		{name: "explicit bin alias", inputPath: "report.txt", inputFormat: "bin", want: analyze.FormatBinary},
		{name: "explicit json", inputPath: "report.txt", inputFormat: analyze.FormatJSON, want: analyze.FormatJSON},
	}

	for _, testCase := range testCases {
		t.Run(testCase.name, func(t *testing.T) {
			t.Parallel()

			got, err := analyze.ResolveInputFormat(testCase.inputPath, testCase.inputFormat)
			require.NoError(t, err)
			require.Equal(t, testCase.want, got)
		})
	}
}

func TestRunCommand_ConvertInput_BinToJSON(t *testing.T) {
	t.Parallel()

	tmpDir := t.TempDir()
	inputPath := filepath.Join(tmpDir, "out.bin")

	var raw bytes.Buffer
	require.NoError(t, defaultStaticAnalyzers()[0].FormatReportBinary(analyze.Report{}, &raw))

	testPipeline := buildPipeline(nil)
	require.NoError(t, testPipeline.Leaves["devs"].Serialize(analyze.Report{}, analyze.FormatBinary, &raw))
	require.NoError(t, os.WriteFile(inputPath, raw.Bytes(), 0o600))

	command := newRunCommandWithDeps(
		func(_ string, _ []string, _ string, _ bool, _ bool, _ io.Writer) error {
			t.Fatal("static executor should not be called in conversion mode")

			return nil
		},
		func(_ context.Context, _ string, _ []string, _ string, _ bool, _ HistoryRunOptions, _ io.Writer) error {
			t.Fatal("history executor should not be called in conversion mode")

			return nil
		},
		stubRunRegistry,
		noopObservabilityInit,
	)

	var out bytes.Buffer
	command.SetOut(&out)
	command.SetArgs([]string{
		"--input", inputPath,
		"--input-format", "bin",
		"--format", "json",
		"-a", "static/complexity,history/devs",
	})

	err := command.Execute()
	require.NoError(t, err)

	var decoded map[string]any
	require.NoError(t, json.Unmarshal(out.Bytes(), &decoded))
	require.Equal(t, "codefang.run.v1", decoded["version"])

	analyzers, ok := decoded["analyzers"].([]any)
	require.True(t, ok)
	require.Len(t, analyzers, 2)
}

func TestRunCommand_ConvertInput_JSONToPlot(t *testing.T) {
	t.Parallel()

	tmpDir := t.TempDir()
	inputPath := filepath.Join(tmpDir, "out.json")

	input := `{
  "version": "codefang.run.v1",
  "analyzers": [
    {
      "id": "static/complexity",
      "mode": "static",
      "report": {"aggregate": {"avg_complexity": 1.5}}
    }
  ]
}`
	require.NoError(t, os.WriteFile(inputPath, []byte(input), 0o600))

	command := newRunCommandWithDeps(
		func(_ string, _ []string, _ string, _ bool, _ bool, _ io.Writer) error {
			t.Fatal("static executor should not be called in conversion mode")

			return nil
		},
		func(_ context.Context, _ string, _ []string, _ string, _ bool, _ HistoryRunOptions, _ io.Writer) error {
			t.Fatal("history executor should not be called in conversion mode")

			return nil
		},
		stubRunRegistry,
		noopObservabilityInit,
	)

	var out bytes.Buffer
	command.SetOut(&out)
	command.SetArgs([]string{
		"--input", inputPath,
		"--input-format", "json",
		"--format", "plot",
		"-a", "static/complexity",
	})

	err := command.Execute()
	require.NoError(t, err)
	require.Contains(t, out.String(), "<!doctype html>")
	require.Contains(t, out.String(), "static/complexity")
}

func TestRunCommand_ConvertInput_BinToPlot(t *testing.T) {
	t.Parallel()

	tmpDir := t.TempDir()
	inputPath := filepath.Join(tmpDir, "out.bin")

	var raw bytes.Buffer
	require.NoError(t, reportutil.EncodeBinaryEnvelope(analyze.Report{"static": true}, &raw))
	require.NoError(t, reportutil.EncodeBinaryEnvelope(analyze.Report{"history": true}, &raw))
	require.NoError(t, os.WriteFile(inputPath, raw.Bytes(), 0o600))

	command := newRunCommandWithDeps(
		func(_ string, _ []string, _ string, _ bool, _ bool, _ io.Writer) error {
			t.Fatal("static executor should not be called in conversion mode")

			return nil
		},
		func(_ context.Context, _ string, _ []string, _ string, _ bool, _ HistoryRunOptions, _ io.Writer) error {
			t.Fatal("history executor should not be called in conversion mode")

			return nil
		},
		stubRunRegistry,
		noopObservabilityInit,
	)

	var out bytes.Buffer
	command.SetOut(&out)
	command.SetArgs([]string{
		"--input", inputPath,
		"--input-format", "bin",
		"--format", "plot",
		"-a", "static/complexity,history/devs",
	})

	err := command.Execute()
	require.NoError(t, err)
	require.Contains(t, out.String(), "<!doctype html>")
	require.Contains(t, out.String(), "static/complexity")
	require.Contains(t, out.String(), "history/devs")
}

func TestRunCommand_MixedPlotRendersCombinedPage(t *testing.T) {
	t.Parallel()

	var (
		staticFormat  string
		historyFormat string
	)

	command := newRunCommandWithDeps(
		func(_ string, ids []string, format string, _ bool, _ bool, writer io.Writer) error {
			staticFormat = format
			require.Equal(t, analyze.FormatBinary, format)
			require.Equal(t, []string{"static/complexity"}, ids)

			return reportutil.EncodeBinaryEnvelope(analyze.Report{"source": "static"}, writer)
		},
		func(_ context.Context, _ string, ids []string, format string, _ bool, _ HistoryRunOptions, writer io.Writer) error {
			historyFormat = format
			require.Equal(t, analyze.FormatBinary, format)
			require.Equal(t, []string{"history/devs"}, ids)

			return reportutil.EncodeBinaryEnvelope(analyze.Report{"source": "history"}, writer)
		},
		stubRunRegistry,
		noopObservabilityInit,
	)

	var out bytes.Buffer
	command.SetOut(&out)
	command.SetArgs([]string{"--format", "plot", "-a", "static/complexity,history/devs", "--path", "."})

	err := command.Execute()
	require.NoError(t, err)
	require.Equal(t, analyze.FormatBinary, staticFormat)
	require.Equal(t, analyze.FormatBinary, historyFormat)
	require.Contains(t, out.String(), "<!doctype html>")
	require.Contains(t, out.String(), "static/complexity")
	require.Contains(t, out.String(), "history/devs")
}

func TestRunCommand_MixedUniversalFormatsRenderUnifiedModel(t *testing.T) {
	t.Parallel()

	testCases := []struct {
		name   string
		format string
	}{
		{name: "json", format: analyze.FormatJSON},
		{name: "yaml", format: analyze.FormatYAML},
		{name: "binary", format: analyze.FormatBinary},
	}

	for _, testCase := range testCases {
		t.Run(testCase.name, func(t *testing.T) {
			t.Parallel()

			var (
				staticFormat  string
				historyFormat string
			)

			command := newRunCommandWithDeps(
				func(_ string, ids []string, format string, _ bool, _ bool, writer io.Writer) error {
					staticFormat = format

					require.Equal(t, []string{"static/complexity"}, ids)

					return reportutil.EncodeBinaryEnvelope(analyze.Report{"source": "static"}, writer)
				},
				func(_ context.Context, _ string, ids []string, format string, _ bool, _ HistoryRunOptions, writer io.Writer) error {
					historyFormat = format

					require.Equal(t, []string{"history/devs"}, ids)

					return reportutil.EncodeBinaryEnvelope(analyze.Report{"source": "history"}, writer)
				},
				stubRunRegistry,
				noopObservabilityInit,
			)

			var out bytes.Buffer
			command.SetOut(&out)
			command.SetArgs([]string{
				"--format", testCase.format,
				"-a", "static/complexity,history/devs",
				"--path", ".",
			})

			err := command.Execute()
			require.NoError(t, err)
			require.Equal(t, analyze.FormatBinary, staticFormat)
			require.Equal(t, analyze.FormatBinary, historyFormat)

			model := decodeMixedOutputToUnifiedModel(t, testCase.format, out.Bytes())
			require.Equal(t, analyze.UnifiedModelVersion, model.Version)
			require.Len(t, model.Analyzers, 2)
			require.Equal(t, "static/complexity", model.Analyzers[0].ID)
			require.Equal(t, analyze.ModeStatic, model.Analyzers[0].Mode)
			require.Equal(t, "static", model.Analyzers[0].Report["source"])
			require.Equal(t, "history/devs", model.Analyzers[1].ID)
			require.Equal(t, analyze.ModeHistory, model.Analyzers[1].Mode)
			require.Equal(t, "history", model.Analyzers[1].Report["source"])
		})
	}
}

func decodeMixedOutputToUnifiedModel(t *testing.T, format string, output []byte) analyze.UnifiedModel {
	t.Helper()

	switch format {
	case analyze.FormatJSON:
		model, err := analyze.ParseUnifiedModelJSON(output)
		require.NoError(t, err)

		return model
	case analyze.FormatYAML:
		model := analyze.UnifiedModel{}
		err := yaml.Unmarshal(output, &model)
		require.NoError(t, err)
		require.NoError(t, model.Validate())

		return model
	case analyze.FormatBinary:
		payload, err := reportutil.DecodeBinaryEnvelope(bytes.NewReader(output))
		require.NoError(t, err)

		model, err := analyze.ParseUnifiedModelJSON(payload)
		require.NoError(t, err)

		return model
	default:
		t.Fatalf("unsupported format: %s", format)

		return analyze.UnifiedModel{}
	}
}

// -------------------------------------------------------------------
// Pipeline assembly tests
// -------------------------------------------------------------------.

func TestPipelineDependencyCompleteness(t *testing.T) {
	t.Parallel()

	pl := buildPipeline(nil)

	for _, analyzer := range pl.Core {
		t.Run("core/"+analyzer.Name(), func(t *testing.T) {
			t.Parallel()
			checkAnalyzerDependencies(t, analyzer)
		})
	}

	for name, analyzer := range pl.Leaves {
		t.Run("leaf/"+name, func(t *testing.T) {
			t.Parallel()
			checkAnalyzerDependencies(t, analyzer)
		})
	}
}

func checkAnalyzerDependencies(t *testing.T, analyzer analyze.HistoryAnalyzer) {
	t.Helper()

	val := reflect.ValueOf(analyzer)
	if val.Kind() == reflect.Ptr {
		val = val.Elem()
	}

	if val.Kind() != reflect.Struct {
		return
	}

	typ := val.Type()

	for i := range val.NumField() {
		field := val.Field(i)
		fieldType := typ.Field(i)

		if !field.CanInterface() {
			continue
		}

		if field.Kind() == reflect.Ptr && isAnalyzerDependency(fieldType.Name) {
			if field.IsNil() {
				t.Errorf("dependency field %s is nil in %s", fieldType.Name, analyzer.Name())
			}
		}
	}
}

func isAnalyzerDependency(fieldName string) bool {
	analyzerDependencies := []string{
		"TreeDiff", "BlobCache", "Identity", "Ticks",
		"FileDiff", "LineStats", "Languages", "UASTChanges",
	}

	return slices.Contains(analyzerDependencies, fieldName)
}

func TestAllAnalyzersSerializeJSON(t *testing.T) {
	t.Parallel()

	pl := buildPipeline(nil)

	for name, analyzer := range pl.Leaves {
		t.Run(name, func(t *testing.T) {
			t.Parallel()

			var buf bytes.Buffer

			err := analyzer.Serialize(analyze.Report{}, analyze.FormatJSON, &buf)
			require.NoError(t, err)
			require.Contains(t, buf.String(), "{")
		})
	}
}

func TestAllAnalyzersSerializeYAML(t *testing.T) {
	t.Parallel()

	pl := buildPipeline(nil)

	for name, analyzer := range pl.Leaves {
		t.Run(name, func(t *testing.T) {
			t.Parallel()

			var buf bytes.Buffer

			err := analyzer.Serialize(analyze.Report{}, analyze.FormatYAML, &buf)
			require.NoError(t, err)
		})
	}
}

func TestAllAnalyzersSerializeBinary(t *testing.T) {
	t.Parallel()

	pl := buildPipeline(nil)

	for name, analyzer := range pl.Leaves {
		t.Run(name, func(t *testing.T) {
			t.Parallel()

			var buf bytes.Buffer

			err := analyzer.Serialize(analyze.Report{}, analyze.FormatBinary, &buf)
			require.NoError(t, err)
			require.NotZero(t, buf.Len())
		})
	}
}

func TestAllAnalyzersRejectUnsupportedFormat(t *testing.T) {
	t.Parallel()

	pl := buildPipeline(nil)

	for name, analyzer := range pl.Leaves {
		t.Run(name, func(t *testing.T) {
			t.Parallel()

			var buf bytes.Buffer

			err := analyzer.Serialize(analyze.Report{}, "html", &buf)
			require.ErrorIs(t, err, analyze.ErrUnsupportedFormat)
		})
	}
}

func TestFormatConstants(t *testing.T) {
	t.Parallel()

	require.Equal(t, "yaml", analyze.FormatYAML)
	require.Equal(t, "json", analyze.FormatJSON)
	require.Equal(t, "binary", analyze.FormatBinary)
	require.Equal(t, "plot", analyze.FormatPlot)
}

func TestRunCommand_DebugTraceFlag_Accepted(t *testing.T) {
	t.Parallel()

	command := newRunCommandWithDeps(
		func(_ string, _ []string, _ string, _ bool, _ bool, _ io.Writer) error {
			return nil
		},
		func(_ context.Context, _ string, _ []string, _ string, _ bool, _ HistoryRunOptions, _ io.Writer) error {
			return nil
		},
		stubRunRegistry,
		noopObservabilityInit,
	)

	command.SetArgs([]string{"-a", "static/complexity", "--debug-trace"})

	err := command.Execute()
	require.NoError(t, err)
}

func TestRunCommand_CreatesRootSpan(t *testing.T) {
	t.Parallel()

	exporter := tracetest.NewInMemoryExporter()
	tp := sdktrace.NewTracerProvider(sdktrace.WithSyncer(exporter))

	t.Cleanup(func() { require.NoError(t, tp.Shutdown(context.Background())) })

	command := newRunCommandWithDeps(
		func(_ string, _ []string, _ string, _ bool, _ bool, _ io.Writer) error {
			return nil
		},
		func(_ context.Context, _ string, _ []string, _ string, _ bool, _ HistoryRunOptions, _ io.Writer) error {
			return nil
		},
		stubRunRegistry,
		func(_ observability.Config) (observability.Providers, error) {
			return observability.Providers{
				Tracer:   tp.Tracer("codefang"),
				Shutdown: func(_ context.Context) error { return nil },
			}, nil
		},
	)

	command.SetArgs([]string{"-a", "static/complexity"})

	err := command.Execute()
	require.NoError(t, err)

	spans := exporter.GetSpans()
	require.NotEmpty(t, spans, "root span should be exported")

	var found bool

	for _, span := range spans {
		if span.Name == "codefang.run" {
			found = true

			break
		}
	}

	require.True(t, found, "root span 'codefang.run' should exist")
}

func TestRunCommand_ShutdownCalledOnExit(t *testing.T) {
	t.Parallel()

	var shutdownCalled bool

	command := newRunCommandWithDeps(
		func(_ string, _ []string, _ string, _ bool, _ bool, _ io.Writer) error {
			return nil
		},
		func(_ context.Context, _ string, _ []string, _ string, _ bool, _ HistoryRunOptions, _ io.Writer) error {
			return nil
		},
		stubRunRegistry,
		func(_ observability.Config) (observability.Providers, error) {
			return observability.Providers{
				Shutdown: func(_ context.Context) error {
					shutdownCalled = true

					return nil
				},
			}, nil
		},
	)

	command.SetArgs([]string{"-a", "static/complexity"})

	err := command.Execute()
	require.NoError(t, err)
	require.True(t, shutdownCalled, "providers.Shutdown must be called on exit")
}

func TestRunCommand_InitializesObservability(t *testing.T) {
	t.Parallel()

	var (
		initCalled bool
		seenCfg    observability.Config
	)

	captureInit := func(cfg observability.Config) (observability.Providers, error) {
		initCalled = true
		seenCfg = cfg

		return observability.Providers{
			Shutdown: func(_ context.Context) error { return nil },
		}, nil
	}

	command := newRunCommandWithDeps(
		func(_ string, _ []string, _ string, _ bool, _ bool, _ io.Writer) error {
			return nil
		},
		func(_ context.Context, _ string, _ []string, _ string, _ bool, _ HistoryRunOptions, _ io.Writer) error {
			return nil
		},
		stubRunRegistry,
		captureInit,
	)

	command.SetArgs([]string{"-a", "static/complexity", "--debug-trace"})

	err := command.Execute()
	require.NoError(t, err)
	require.True(t, initCalled, "observability.Init should be called")
	require.Equal(t, observability.ModeCLI, seenCfg.Mode)
	require.True(t, seenCfg.DebugTrace, "DebugTrace should be true when --debug-trace is set")
	require.NotEmpty(t, seenCfg.ServiceVersion, "ServiceVersion should be set")
}

func stubRunRegistry() (*analyze.Registry, error) {
	staticAnalyzers := []analyze.StaticAnalyzer{
		&stubStaticRunAnalyzer{
			descriptor: analyze.Descriptor{
				ID:          "static/complexity",
				Description: "complexity",
				Mode:        analyze.ModeStatic,
			},
		},
	}

	historyAnalyzers := []analyze.HistoryAnalyzer{
		&stubHistoryRunAnalyzer{
			descriptor: analyze.Descriptor{
				ID:          "history/devs",
				Description: "devs",
				Mode:        analyze.ModeHistory,
			},
		},
	}

	return analyze.NewRegistry(staticAnalyzers, historyAnalyzers)
}

func noopObservabilityInit(_ observability.Config) (observability.Providers, error) {
	return observability.Providers{
		Shutdown: func(_ context.Context) error { return nil },
	}, nil
}

func TestDurationClass(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name string
		d    time.Duration
		want string
	}{
		{"fast", 5 * time.Second, durationClassFast},
		{"normal", 30 * time.Second, durationClassNormal},
		{"slow", 2 * time.Minute, durationClassSlow},
		{"zero is fast", 0, durationClassFast},
		{"boundary fast", durationClassFastLimit - 1, durationClassFast},
		{"boundary normal", durationClassNormalLimit - 1, durationClassNormal},
		{"exact fast limit", durationClassFastLimit, durationClassNormal},
		{"exact normal limit", durationClassNormalLimit, durationClassSlow},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			got := durationClass(tt.d)
			if got != tt.want {
				t.Fatalf("durationClass(%v) = %q, want %q", tt.d, got, tt.want)
			}
		})
	}
}

func TestRunCommand_RootSpanAttributes(t *testing.T) {
	t.Parallel()

	exporter := tracetest.NewInMemoryExporter()
	tp := sdktrace.NewTracerProvider(sdktrace.WithSyncer(exporter))

	t.Cleanup(func() { require.NoError(t, tp.Shutdown(context.Background())) })

	command := newRunCommandWithDeps(
		func(_ string, _ []string, _ string, _ bool, _ bool, _ io.Writer) error {
			return nil
		},
		func(_ context.Context, _ string, _ []string, _ string, _ bool, _ HistoryRunOptions, _ io.Writer) error {
			return nil
		},
		stubRunRegistry,
		func(_ observability.Config) (observability.Providers, error) {
			return observability.Providers{
				Tracer:   tp.Tracer("codefang"),
				Shutdown: func(_ context.Context) error { return nil },
			}, nil
		},
	)

	command.SetArgs([]string{"-a", "static/complexity"})

	err := command.Execute()
	require.NoError(t, err)

	spans := exporter.GetSpans()

	var rootAttrs map[string]any

	for _, span := range spans {
		if span.Name != "codefang.run" {
			continue
		}

		rootAttrs = make(map[string]any, len(span.Attributes))
		for _, attr := range span.Attributes {
			rootAttrs[string(attr.Key)] = attr.Value.AsInterface()
		}
	}

	require.NotNil(t, rootAttrs, "root span should exist")
	require.Contains(t, rootAttrs, "error", "root span should have error attribute")
	require.Equal(t, false, rootAttrs["error"], "error should be false on success")
	require.Contains(t, rootAttrs, "codefang.duration_class", "root span should have duration_class")
}
