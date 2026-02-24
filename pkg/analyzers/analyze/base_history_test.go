package analyze_test

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"testing"

	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/common/reportutil"
	"github.com/Sumatoshi-tech/codefang/pkg/pipeline"
)

var errMock = errors.New("mock error")

type DummyMetrics struct {
	Name  string `json:"name"  yaml:"name"`
	Count int    `json:"count" yaml:"count"`
}

func computeDummyMetrics(r analyze.Report) (*DummyMetrics, error) {
	if val, ok := r["error"].(bool); ok && val {
		return nil, errMock
	}

	return &DummyMetrics{
		Count: 42,
		Name:  "dummy",
	}, nil
}

func dummyTicksToReport(_ context.Context, ticks []analyze.TICK) analyze.Report {
	if len(ticks) == 0 {
		return analyze.Report{}
	}

	return analyze.Report{"ticks_len": len(ticks)}
}

func TestBaseHistoryAnalyzer_Metadata(t *testing.T) {
	t.Parallel()

	opts := []pipeline.ConfigurationOption{{Name: "test-opt"}}
	base := &analyze.BaseHistoryAnalyzer[*DummyMetrics]{
		Desc: analyze.Descriptor{
			ID:          "history/dummy",
			Description: "Dummy analyzer",
		},
		Sequential:         true,
		CPUHeavyFlag:       false,
		EstimatedStateSize: 1024,
		EstimatedTCSize:    256,
		ConfigOptions:      opts,
	}

	require.Equal(t, "history/dummy", base.Name())
	require.Equal(t, "dummy", base.Flag())
	require.Equal(t, "Dummy analyzer", base.Description())
	require.Equal(t, "history/dummy", base.Descriptor().ID)
	require.True(t, base.SequentialOnly())
	require.False(t, base.CPUHeavy())
	require.Equal(t, int64(1024), base.WorkingStateSize())
	require.Equal(t, int64(256), base.AvgTCSize())
	require.Equal(t, opts, base.ListConfigurationOptions())
	require.NoError(t, base.Configure(nil))
}

func TestBaseHistoryAnalyzer_FlagNoSlash(t *testing.T) {
	t.Parallel()

	base := &analyze.BaseHistoryAnalyzer[*DummyMetrics]{
		Desc: analyze.Descriptor{ID: "dummy"},
	}

	require.Equal(t, "dummy", base.Flag())
}

func TestBaseHistoryAnalyzer_Serialize(t *testing.T) {
	t.Parallel()

	base := &analyze.BaseHistoryAnalyzer[*DummyMetrics]{
		ComputeMetricsFn: computeDummyMetrics,
	}

	t.Run("JSON", func(t *testing.T) {
		t.Parallel()

		var buf bytes.Buffer

		err := base.Serialize(analyze.Report{}, analyze.FormatJSON, &buf)
		require.NoError(t, err)

		var m DummyMetrics

		err = json.Unmarshal(buf.Bytes(), &m)
		require.NoError(t, err)
		require.Equal(t, 42, m.Count)
		require.Equal(t, "dummy", m.Name)
	})

	t.Run("YAML", func(t *testing.T) {
		t.Parallel()

		var buf bytes.Buffer

		err := base.Serialize(analyze.Report{}, analyze.FormatYAML, &buf)
		require.NoError(t, err)

		require.Contains(t, buf.String(), "count: 42")
		require.Contains(t, buf.String(), "name: dummy")
	})

	t.Run("Binary", func(t *testing.T) {
		t.Parallel()

		var buf bytes.Buffer

		err := base.Serialize(analyze.Report{}, analyze.FormatBinary, &buf)
		require.NoError(t, err)

		payload, err := reportutil.DecodeBinaryEnvelope(&buf)
		require.NoError(t, err)

		var m DummyMetrics

		err = json.Unmarshal(payload, &m)
		require.NoError(t, err)
		require.Equal(t, 42, m.Count)
	})

	t.Run("Unsupported Format", func(t *testing.T) {
		t.Parallel()

		var buf bytes.Buffer

		err := base.Serialize(analyze.Report{}, "unsupported", &buf)
		require.ErrorIs(t, err, analyze.ErrUnsupportedFormat)
	})

	t.Run("ComputeError", func(t *testing.T) {
		t.Parallel()

		var buf bytes.Buffer

		err := base.Serialize(analyze.Report{"error": true}, analyze.FormatJSON, &buf)
		require.ErrorContains(t, err, "mock error")
	})

	t.Run("MissingHook", func(t *testing.T) {
		t.Parallel()

		baseNoHook := &analyze.BaseHistoryAnalyzer[*DummyMetrics]{}

		var buf bytes.Buffer

		err := baseNoHook.Serialize(analyze.Report{}, analyze.FormatJSON, &buf)
		require.ErrorIs(t, err, analyze.ErrMissingComputeMetrics)
	})
}

func TestBaseHistoryAnalyzer_SerializeTICKs(t *testing.T) {
	t.Parallel()

	base := &analyze.BaseHistoryAnalyzer[*DummyMetrics]{
		ComputeMetricsFn: computeDummyMetrics,
		TicksToReportFn:  dummyTicksToReport,
	}

	ticks := []analyze.TICK{{Tick: 1}, {Tick: 2}}

	var buf bytes.Buffer

	err := base.SerializeTICKs(ticks, analyze.FormatJSON, &buf)
	require.NoError(t, err)

	var m DummyMetrics

	err = json.Unmarshal(buf.Bytes(), &m)
	require.NoError(t, err)
	require.Equal(t, 42, m.Count)

	t.Run("MissingHook", func(t *testing.T) {
		t.Parallel()

		baseNoHook := &analyze.BaseHistoryAnalyzer[*DummyMetrics]{}

		var b bytes.Buffer

		err = baseNoHook.SerializeTICKs(ticks, analyze.FormatJSON, &b)
		require.ErrorIs(t, err, analyze.ErrNotImplemented)
	})
}

func TestBaseHistoryAnalyzer_Snapshots(t *testing.T) {
	t.Parallel()

	base := &analyze.BaseHistoryAnalyzer[*DummyMetrics]{}

	// Should not panic, return nil or perform no-ops.
	snap := base.SnapshotPlumbing()
	require.Nil(t, snap)

	base.ApplySnapshot(snap)
	base.ReleaseSnapshot(snap)
}
