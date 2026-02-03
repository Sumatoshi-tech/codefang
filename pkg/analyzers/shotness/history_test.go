package shotness //nolint:testpackage // testing internal implementation.

import (
	"bytes"
	"encoding/json"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
)

func TestHistoryAnalyzer_Name(t *testing.T) {
	t.Parallel()

	s := &HistoryAnalyzer{}
	if s.Name() == "" {
		t.Error("Name empty")
	}
}

func TestHistoryAnalyzer_Flag(t *testing.T) {
	t.Parallel()

	s := &HistoryAnalyzer{}
	if s.Flag() == "" {
		t.Error("Flag empty")
	}
}

func TestHistoryAnalyzer_Description(t *testing.T) {
	t.Parallel()

	s := &HistoryAnalyzer{}
	if s.Description() == "" {
		t.Error("Description empty")
	}
}

func TestHistoryAnalyzer_ListConfigurationOptions(t *testing.T) {
	t.Parallel()

	s := &HistoryAnalyzer{}
	opts := s.ListConfigurationOptions()
	// May or may not have options.
	_ = opts
}

func TestHistoryAnalyzer_Configure(t *testing.T) {
	t.Parallel()

	s := &HistoryAnalyzer{}
	err := s.Configure(nil)
	require.NoError(t, err)
}

func TestHistoryAnalyzer_Initialize(t *testing.T) {
	t.Parallel()

	s := &HistoryAnalyzer{}
	err := s.Initialize(nil)
	require.NoError(t, err)
}

func TestHistoryAnalyzer_Fork(t *testing.T) {
	t.Parallel()

	s := &HistoryAnalyzer{}
	require.NoError(t, s.Initialize(nil))

	clones := s.Fork(2)
	if len(clones) != 2 {
		t.Error("expected 2 clones")
	}
}

func TestHistoryAnalyzer_Serialize_JSON_UsesComputedMetrics(t *testing.T) {
	t.Parallel()

	s := &HistoryAnalyzer{}

	nodes := []NodeSummary{
		{Type: "Function", Name: "testFunc", File: "test.go"},
	}
	counters := []map[int]int{
		{0: 15},
	}

	report := analyze.Report{
		"Nodes":    nodes,
		"Counters": counters,
	}

	var buf bytes.Buffer
	err := s.Serialize(report, analyze.FormatJSON, &buf)
	require.NoError(t, err)

	var result map[string]any
	err = json.Unmarshal(buf.Bytes(), &result)
	require.NoError(t, err)

	// Should have computed metrics structure
	assert.Contains(t, result, "node_hotness")
	assert.Contains(t, result, "node_coupling")
	assert.Contains(t, result, "hotspot_nodes")
	assert.Contains(t, result, "aggregate")
}

func TestHistoryAnalyzer_Serialize_YAML_UsesComputedMetrics(t *testing.T) {
	t.Parallel()

	s := &HistoryAnalyzer{}

	nodes := []NodeSummary{
		{Type: "Function", Name: "testFunc", File: "test.go"},
	}
	counters := []map[int]int{
		{0: 15},
	}

	report := analyze.Report{
		"Nodes":    nodes,
		"Counters": counters,
	}

	var buf bytes.Buffer
	err := s.Serialize(report, analyze.FormatYAML, &buf)
	require.NoError(t, err)

	output := buf.String()
	// Should have computed metrics structure (YAML keys)
	assert.Contains(t, output, "node_hotness:")
	assert.Contains(t, output, "node_coupling:")
	assert.Contains(t, output, "hotspot_nodes:")
	assert.Contains(t, output, "aggregate:")
}
