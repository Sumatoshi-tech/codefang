package analyze_test

import (
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
)

const (
	testAnalyzerComplexity = "static/complexity"
	testAnalyzerSentiment  = "history/sentiment"
	testFieldFunctions     = "function_complexity"
	testFieldTimeSeries    = "time_series"
)

func TestSchemaForAnalyzer_Known(t *testing.T) {
	t.Parallel()

	schema := analyze.SchemaForAnalyzer(testAnalyzerComplexity)

	require.NotNil(t, schema)
	assert.Contains(t, schema, testFieldFunctions)
	assert.Equal(t, "list", schema[testFieldFunctions].Type)
	assert.Equal(t, "function", schema[testFieldFunctions].Grain)
}

func TestSchemaForAnalyzer_HistoryAnalyzer(t *testing.T) {
	t.Parallel()

	schema := analyze.SchemaForAnalyzer(testAnalyzerSentiment)

	require.NotNil(t, schema)
	assert.Contains(t, schema, testFieldTimeSeries)
	assert.Equal(t, "time_series", schema[testFieldTimeSeries].Type)
	assert.Equal(t, "tick", schema[testFieldTimeSeries].Grain)
}

func TestSchemaForAnalyzer_Unknown(t *testing.T) {
	t.Parallel()

	schema := analyze.SchemaForAnalyzer("unknown/analyzer")

	assert.Nil(t, schema)
}

func TestSchemaForAnalyzer_AllRegistered(t *testing.T) {
	t.Parallel()

	knownIDs := []string{
		"static/complexity", "static/halstead", "static/cohesion",
		"static/comments", "static/clones", "static/imports",
		"static/composition",
		"history/sentiment", "history/anomaly", "history/devs",
		"history/file-history", "history/couples", "history/shotness",
		"history/burndown", "history/quality", "history/imports",
		"history/typos",
	}

	for _, id := range knownIDs {
		schema := analyze.SchemaForAnalyzer(id)
		assert.NotNilf(t, schema, "schema missing for %s", id)
	}
}
