package burndown //nolint:testpackage // testing internal implementation.

import (
	"bytes"
	"encoding/json"
	"testing"
	"time"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
)

func TestHistoryAnalyzer_Name(t *testing.T) {
	t.Parallel()

	b := &HistoryAnalyzer{}
	if b.Name() == "" {
		t.Error("Name empty")
	}
}

func TestHistoryAnalyzer_Flag(t *testing.T) {
	t.Parallel()

	b := &HistoryAnalyzer{}
	if b.Flag() == "" {
		t.Error("Flag empty")
	}
}

func TestHistoryAnalyzer_Description(t *testing.T) {
	t.Parallel()

	b := &HistoryAnalyzer{}
	if b.Description() == "" {
		t.Error("Description empty")
	}
}

func TestHistoryAnalyzer_ListConfigurationOptions(t *testing.T) {
	t.Parallel()

	b := &HistoryAnalyzer{}

	opts := b.ListConfigurationOptions()
	if len(opts) == 0 {
		t.Error("expected options")
	}
}

func TestHistoryAnalyzer_Configure(t *testing.T) {
	t.Parallel()

	b := &HistoryAnalyzer{}
	err := b.Configure(nil)
	require.NoError(t, err)
}

func TestHistoryAnalyzer_Initialize(t *testing.T) {
	t.Parallel()

	b := &HistoryAnalyzer{
		Granularity: 30,
		Sampling:    30,
		Goroutines:  4,
	}
	err := b.Initialize(nil)
	require.NoError(t, err)
}

func TestHistoryAnalyzer_Serialize_JSON_UsesComputedMetrics(t *testing.T) {
	t.Parallel()

	b := &HistoryAnalyzer{}

	report := analyze.Report{
		"GlobalHistory":      DenseHistory{{100, 200}, {150, 180}},
		"FileHistories":      map[string]DenseHistory{"main.go": {{50, 100}}},
		"FileOwnership":      map[string]map[int]int{"main.go": {0: 100}},
		"PeopleHistories":    []DenseHistory{{{100, 200}}},
		"ReversedPeopleDict": []string{"Alice"},
		"TickSize":           24 * time.Hour,
		"Sampling":           30,
	}

	var buf bytes.Buffer
	err := b.Serialize(report, analyze.FormatJSON, &buf)
	require.NoError(t, err)

	var result map[string]any
	err = json.Unmarshal(buf.Bytes(), &result)
	require.NoError(t, err)

	// Should have computed metrics structure
	assert.Contains(t, result, "aggregate")
	assert.Contains(t, result, "global_survival")
	assert.Contains(t, result, "file_survival")
	assert.Contains(t, result, "developer_survival")
	assert.Contains(t, result, "interactions")
}

func TestHistoryAnalyzer_Serialize_YAML_UsesComputedMetrics(t *testing.T) {
	t.Parallel()

	b := &HistoryAnalyzer{}

	report := analyze.Report{
		"GlobalHistory":      DenseHistory{{100, 200}, {150, 180}},
		"FileHistories":      map[string]DenseHistory{"main.go": {{50, 100}}},
		"FileOwnership":      map[string]map[int]int{"main.go": {0: 100}},
		"PeopleHistories":    []DenseHistory{{{100, 200}}},
		"ReversedPeopleDict": []string{"Alice"},
		"TickSize":           24 * time.Hour,
		"Sampling":           30,
	}

	var buf bytes.Buffer
	err := b.Serialize(report, analyze.FormatYAML, &buf)
	require.NoError(t, err)

	output := buf.String()
	// Should have computed metrics structure (YAML keys)
	assert.Contains(t, output, "aggregate:")
	assert.Contains(t, output, "global_survival:")
	assert.Contains(t, output, "file_survival:")
	assert.Contains(t, output, "developer_survival:")
	assert.Contains(t, output, "interactions:")
}
