package typos //nolint:testpackage // testing internal implementation.

import (
	"bytes"
	"encoding/json"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/assert/yaml"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

func TestHistoryAnalyzer_Name(t *testing.T) {
	t.Parallel()

	h := &HistoryAnalyzer{}
	if h.Name() == "" {
		t.Error("Name empty")
	}
}

func TestHistoryAnalyzer_Flag(t *testing.T) {
	t.Parallel()

	h := &HistoryAnalyzer{}
	if h.Flag() == "" {
		t.Error("Flag empty")
	}
}

func TestHistoryAnalyzer_Description(t *testing.T) {
	t.Parallel()

	h := &HistoryAnalyzer{}
	if h.Description() == "" {
		t.Error("Description empty")
	}
}

func TestHistoryAnalyzer_ListConfigurationOptions(t *testing.T) {
	t.Parallel()

	h := &HistoryAnalyzer{}
	opts := h.ListConfigurationOptions()
	_ = opts
}

func TestHistoryAnalyzer_Configure(t *testing.T) {
	t.Parallel()

	h := &HistoryAnalyzer{}
	err := h.Configure(nil)
	require.NoError(t, err)
}

func TestHistoryAnalyzer_Initialize(t *testing.T) {
	t.Parallel()

	h := &HistoryAnalyzer{}
	err := h.Initialize(nil)
	require.NoError(t, err)
}

func TestHistoryAnalyzer_Fork(t *testing.T) {
	t.Parallel()

	h := &HistoryAnalyzer{}
	require.NoError(t, h.Initialize(nil))

	clones := h.Fork(2)
	if len(clones) != 2 {
		t.Error("expected 2 clones")
	}
}

// --- Serialize Tests ---

func TestHistoryAnalyzer_Serialize_JSON(t *testing.T) {
	t.Parallel()

	h := &HistoryAnalyzer{}
	typos := []Typo{
		{Wrong: "tets", Correct: "test", File: "main.go", Line: 10, Commit: gitlib.Hash{}},
		{Wrong: "functon", Correct: "function", File: "util.go", Line: 20, Commit: gitlib.Hash{}},
	}
	report := analyze.Report{"typos": typos}

	var buf bytes.Buffer
	err := h.Serialize(report, analyze.FormatJSON, &buf)

	require.NoError(t, err)

	// Verify output is valid JSON
	var result ComputedMetrics
	err = json.Unmarshal(buf.Bytes(), &result)
	require.NoError(t, err)

	// Verify metrics structure
	assert.Len(t, result.TypoList, 2)
	assert.Equal(t, 2, result.Aggregate.TotalTypos)
}

func TestHistoryAnalyzer_Serialize_JSON_Empty(t *testing.T) {
	t.Parallel()

	h := &HistoryAnalyzer{}
	report := analyze.Report{}

	var buf bytes.Buffer
	err := h.Serialize(report, analyze.FormatJSON, &buf)

	require.NoError(t, err)

	// Verify output is valid JSON
	var result ComputedMetrics
	err = json.Unmarshal(buf.Bytes(), &result)
	require.NoError(t, err)

	assert.Empty(t, result.TypoList)
	assert.Equal(t, 0, result.Aggregate.TotalTypos)
}

func TestHistoryAnalyzer_Serialize_YAML(t *testing.T) {
	t.Parallel()

	h := &HistoryAnalyzer{}
	typos := []Typo{
		{Wrong: "tets", Correct: "test", File: "main.go", Line: 10, Commit: gitlib.Hash{}},
		{Wrong: "functon", Correct: "function", File: "util.go", Line: 20, Commit: gitlib.Hash{}},
	}
	report := analyze.Report{"typos": typos}

	var buf bytes.Buffer
	err := h.Serialize(report, analyze.FormatYAML, &buf)

	require.NoError(t, err)

	// Verify output is valid YAML
	var result ComputedMetrics
	err = yaml.Unmarshal(buf.Bytes(), &result)
	require.NoError(t, err)

	// Verify metrics structure
	assert.Len(t, result.TypoList, 2)
	assert.Equal(t, 2, result.Aggregate.TotalTypos)
}

func TestHistoryAnalyzer_Serialize_YAML_Empty(t *testing.T) {
	t.Parallel()

	h := &HistoryAnalyzer{}
	report := analyze.Report{}

	var buf bytes.Buffer
	err := h.Serialize(report, analyze.FormatYAML, &buf)

	require.NoError(t, err)

	// Verify output is valid YAML
	var result ComputedMetrics
	err = yaml.Unmarshal(buf.Bytes(), &result)
	require.NoError(t, err)

	assert.Empty(t, result.TypoList)
	assert.Equal(t, 0, result.Aggregate.TotalTypos)
}

func TestHistoryAnalyzer_Serialize_YAML_ContainsExpectedFields(t *testing.T) {
	t.Parallel()

	h := &HistoryAnalyzer{}
	typos := []Typo{
		{Wrong: "tets", Correct: "test", File: "main.go", Line: 10, Commit: gitlib.Hash{}},
	}
	report := analyze.Report{"typos": typos}

	var buf bytes.Buffer
	err := h.Serialize(report, analyze.FormatYAML, &buf)

	require.NoError(t, err)

	output := buf.String()
	assert.Contains(t, output, "typo_list:")
	assert.Contains(t, output, "patterns:")
	assert.Contains(t, output, "file_typos:")
	assert.Contains(t, output, "aggregate:")
}

func TestHistoryAnalyzer_Serialize_DefaultFormat(t *testing.T) {
	t.Parallel()

	h := &HistoryAnalyzer{}
	report := analyze.Report{}

	var buf bytes.Buffer
	err := h.Serialize(report, "", &buf)

	require.NoError(t, err)

	// Default should be YAML
	var result ComputedMetrics
	err = yaml.Unmarshal(buf.Bytes(), &result)
	require.NoError(t, err)
}
