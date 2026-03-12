package renderer

import (
	"bytes"
	"encoding/json"
	"testing"

	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
)

func ParseUnifiedModelJSON(data []byte) (UnifiedModel, error) {
	return analyze.ParseUnifiedModelJSON(data)
}

func TestParseUnifiedModelJSON(t *testing.T) {
	t.Parallel()

	input := UnifiedModel{
		Version: UnifiedModelVersion,
		Analyzers: []AnalyzerResult{
			{
				ID:   "static/complexity",
				Mode: analyze.ModeStatic,
				Report: analyze.Report{
					"aggregate": map[string]any{"avg_complexity": 1.5},
				},
			},
		},
	}

	data, err := json.Marshal(input)
	require.NoError(t, err)

	parsed, err := ParseUnifiedModelJSON(data)
	require.NoError(t, err)
	require.Equal(t, input.Version, parsed.Version)
	require.Len(t, parsed.Analyzers, 1)
	require.Equal(t, "static/complexity", parsed.Analyzers[0].ID)
}

func TestParseUnifiedModelJSON_RejectsInvalid(t *testing.T) {
	t.Parallel()

	input := []byte(`{"version":"codefang.run.v1","analyzers":[{"id":"","mode":"static","report":{}}]}`)
	_, err := ParseUnifiedModelJSON(input)
	require.ErrorIs(t, err, ErrInvalidUnifiedModel)
}

func TestRenderUnifiedModelPlot(t *testing.T) {
	t.Parallel()

	model := UnifiedModel{
		Version: UnifiedModelVersion,
		Analyzers: []AnalyzerResult{
			{
				ID:   "history/devs",
				Mode: analyze.ModeHistory,
				Report: analyze.Report{
					"aggregate": map[string]any{"authors": 3},
				},
			},
		},
	}

	var buf bytes.Buffer

	err := RenderUnifiedModelPlot(model, &buf)
	require.NoError(t, err)
	require.Contains(t, buf.String(), "<!doctype html>")
	require.Contains(t, buf.String(), "history/devs")
}
