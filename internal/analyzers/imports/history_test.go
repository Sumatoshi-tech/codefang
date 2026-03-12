package imports

import (
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
)

func TestHistoryAnalyzer_Name(t *testing.T) {
	t.Parallel()

	h := NewHistoryAnalyzer()
	if h.Name() == "" {
		t.Error("Name empty")
	}
}

func TestHistoryAnalyzer_Flag(t *testing.T) {
	t.Parallel()

	h := NewHistoryAnalyzer()
	if h.Flag() == "" {
		t.Error("Flag empty")
	}
}

func TestHistoryAnalyzer_Configure(t *testing.T) {
	t.Parallel()

	h := NewHistoryAnalyzer()
	err := h.Configure(nil)
	require.NoError(t, err)
}

func TestHistoryAnalyzer_ListConfigurationOptions(t *testing.T) {
	t.Parallel()

	h := NewHistoryAnalyzer()

	// Imports consumes UAST from the framework pipeline; no per-analyzer config options.
	opts := h.ListConfigurationOptions()
	assert.Empty(t, opts)
}

func TestExtractCommitTimeSeries(t *testing.T) {
	t.Parallel()

	h := NewHistoryAnalyzer()

	hashStr := "aabbccdd00112233445566778899aabbccddeeff"
	report := analyze.Report{
		"commit_stats": map[string]*CommitSummary{
			hashStr: {
				ImportCount: 5,
				Languages:   map[string]int{"go": 3, "python": 2},
			},
		},
	}

	result := h.ExtractCommitTimeSeries(report)
	require.NotNil(t, result)
	require.Contains(t, result, hashStr)

	entry, ok := result[hashStr].(map[string]any)
	require.True(t, ok)
	assert.Equal(t, 5, entry["import_count"])

	langs, ok := entry["languages"].(map[string]int)
	require.True(t, ok)
	assert.Equal(t, 3, langs["go"])
	assert.Equal(t, 2, langs["python"])
}

func TestExtractCommitTimeSeries_Empty(t *testing.T) {
	t.Parallel()

	h := NewHistoryAnalyzer()

	result := h.ExtractCommitTimeSeries(analyze.Report{})
	assert.Nil(t, result)

	result = h.ExtractCommitTimeSeries(analyze.Report{
		"commit_stats": map[string]*CommitSummary{},
	})
	assert.Nil(t, result)
}
