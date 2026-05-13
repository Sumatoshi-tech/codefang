package analyze_test

import (
	"bytes"
	"encoding/json"
	"strings"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
)

func TestWriteConvertedOutput_NDJSON_OneLinePerAnalyzer(t *testing.T) {
	t.Parallel()

	model := analyze.UnifiedModel{
		Version: analyze.UnifiedModelVersion,
		Analyzers: []analyze.AnalyzerResult{
			{ID: "static/complexity", Mode: analyze.ModeStatic, Report: analyze.Report{"total": 10}},
			{ID: "history/sentiment", Mode: analyze.ModeHistory, Report: analyze.Report{"score": 0.8}},
		},
	}

	var buf bytes.Buffer

	err := analyze.WriteConvertedOutput(model, analyze.FormatNDJSON, &buf)
	require.NoError(t, err)

	lines := strings.Split(strings.TrimSpace(buf.String()), "\n")
	require.Len(t, lines, 2)

	var line1 map[string]any
	require.NoError(t, json.Unmarshal([]byte(lines[0]), &line1))
	assert.Equal(t, "static/complexity", line1["id"])
	assert.Equal(t, "static", line1["mode"])

	var line2 map[string]any
	require.NoError(t, json.Unmarshal([]byte(lines[1]), &line2))
	assert.Equal(t, "history/sentiment", line2["id"])
}

func TestWriteConvertedOutput_NDJSON_EmptyAnalyzers(t *testing.T) {
	t.Parallel()

	model := analyze.UnifiedModel{
		Version:   analyze.UnifiedModelVersion,
		Analyzers: nil,
	}

	var buf bytes.Buffer

	err := analyze.WriteConvertedOutput(model, analyze.FormatNDJSON, &buf)
	require.NoError(t, err)

	assert.Empty(t, strings.TrimSpace(buf.String()))
}

func TestWriteConvertedOutput_NDJSON_WithMetadata(t *testing.T) {
	t.Parallel()

	model := analyze.UnifiedModel{
		Version:  analyze.UnifiedModelVersion,
		Metadata: analyze.NewAnalysisMetadata("/repo/test"),
		Analyzers: []analyze.AnalyzerResult{
			{ID: "static/test", Mode: analyze.ModeStatic, Report: analyze.Report{}},
		},
	}

	var buf bytes.Buffer

	err := analyze.WriteConvertedOutput(model, analyze.FormatNDJSON, &buf)
	require.NoError(t, err)

	lines := strings.Split(strings.TrimSpace(buf.String()), "\n")
	require.Len(t, lines, 2) // Metadata line + 1 analyzer line.

	var metaLine map[string]any
	require.NoError(t, json.Unmarshal([]byte(lines[0]), &metaLine))
	assert.Equal(t, analyze.UnifiedModelVersion, metaLine["version"])
	assert.NotNil(t, metaLine["metadata"])
}
