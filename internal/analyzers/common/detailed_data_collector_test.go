package common

import (
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
)

// FRD: specs/frds/FRD-20260303-detailed-data-collector.md.

func TestNewDetailedDataCollector(t *testing.T) {
	t.Parallel()

	t.Run("single_key", func(t *testing.T) {
		t.Parallel()

		d := NewDetailedDataCollector("functions")
		require.NotNil(t, d)
		assert.Equal(t, []string{"functions"}, d.keys)
		assert.Empty(t, d.collections["functions"])
	})

	t.Run("multiple_keys", func(t *testing.T) {
		t.Parallel()

		d := NewDetailedDataCollector("comments", "functions")
		require.NotNil(t, d)
		assert.Equal(t, []string{"comments", "functions"}, d.keys)
		assert.Empty(t, d.collections["comments"])
		assert.Empty(t, d.collections["functions"])
	})
}

func TestDetailedDataCollector_CollectFromReports_SingleKey(t *testing.T) {
	t.Parallel()

	d := NewDetailedDataCollector("functions")

	results := map[string]analyze.Report{
		"file1": {
			"functions": []map[string]any{
				{"name": "func1"},
			},
		},
		"file2": {
			"functions": []map[string]any{
				{"name": "func2"},
				{"name": "func3"},
			},
		},
	}

	d.CollectFromReports(results)

	assert.Len(t, d.collections["functions"], 3)
}

func TestDetailedDataCollector_CollectFromReports_NilReportsSkipped(t *testing.T) {
	t.Parallel()

	d := NewDetailedDataCollector("functions")

	results := map[string]analyze.Report{
		"file1": {
			"functions": []map[string]any{
				{"name": "func1"},
			},
		},
		"file2": nil,
	}

	d.CollectFromReports(results)

	assert.Len(t, d.collections["functions"], 1)
}

func TestDetailedDataCollector_CollectFromReports_NoMatchingKey(t *testing.T) {
	t.Parallel()

	d := NewDetailedDataCollector("functions")

	results := map[string]analyze.Report{
		"file1": {
			"total_functions": 0,
		},
	}

	d.CollectFromReports(results)

	assert.Empty(t, d.collections["functions"])
}

func TestDetailedDataCollector_CollectFromReports_WrongType(t *testing.T) {
	t.Parallel()

	d := NewDetailedDataCollector("functions")

	results := map[string]analyze.Report{
		"file1": {
			"functions": "not a slice",
		},
	}

	d.CollectFromReports(results)

	assert.Empty(t, d.collections["functions"])
}

func TestDetailedDataCollector_CollectFromReports_MultiKey(t *testing.T) {
	t.Parallel()

	d := NewDetailedDataCollector("comments", "functions")

	results := map[string]analyze.Report{
		"file1": {
			"comments": []map[string]any{
				{"line": 1, "text": "good comment"},
			},
			"functions": []map[string]any{
				{"name": "func1"},
			},
		},
		"file2": {
			"comments": []map[string]any{
				{"line": 5, "text": "another comment"},
				{"line": 10, "text": "third comment"},
			},
			"functions": []map[string]any{
				{"name": "func2"},
			},
		},
	}

	d.CollectFromReports(results)

	assert.Len(t, d.collections["comments"], 3)
	assert.Len(t, d.collections["functions"], 2)
}

func TestDetailedDataCollector_AddToResult_NonEmpty(t *testing.T) {
	t.Parallel()

	d := NewDetailedDataCollector("functions")

	results := map[string]analyze.Report{
		"file1": {
			"functions": []map[string]any{
				{"name": "func1"},
				{"name": "func2"},
			},
		},
	}

	d.CollectFromReports(results)

	result := analyze.Report{}
	d.AddToResult(result)

	functions, ok := result["functions"].([]map[string]any)
	require.True(t, ok)
	assert.Len(t, functions, 2)
}

func TestDetailedDataCollector_AddToResult_EmptySkipped(t *testing.T) {
	t.Parallel()

	d := NewDetailedDataCollector("functions")

	result := analyze.Report{}
	d.AddToResult(result)

	_, hasKey := result["functions"]
	assert.False(t, hasKey)
}

func TestDetailedDataCollector_AddToResult_MultiKey_Partial(t *testing.T) {
	t.Parallel()

	d := NewDetailedDataCollector("comments", "functions")

	results := map[string]analyze.Report{
		"file1": {
			"comments": []map[string]any{
				{"line": 1},
			},
		},
	}

	d.CollectFromReports(results)

	result := analyze.Report{}
	d.AddToResult(result)

	_, hasComments := result["comments"]
	assert.True(t, hasComments)

	_, hasFunctions := result["functions"]
	assert.False(t, hasFunctions)
}

func TestDetailedDataCollector_EmptyResults(t *testing.T) {
	t.Parallel()

	d := NewDetailedDataCollector("functions")

	d.CollectFromReports(map[string]analyze.Report{})

	result := analyze.Report{}
	d.AddToResult(result)

	assert.Empty(t, result)
}
