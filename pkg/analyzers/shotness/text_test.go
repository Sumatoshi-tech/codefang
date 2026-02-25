package shotness

import (
	"bytes"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
)

func TestGenerateText_Summary(t *testing.T) {
	t.Parallel()

	report := buildTestReport()
	s := NewAnalyzer()

	var buf bytes.Buffer

	err := s.generateText(report, &buf)
	require.NoError(t, err)

	output := buf.String()
	assert.Contains(t, output, "Shotness Analysis")
	assert.Contains(t, output, "Summary")
	assert.Contains(t, output, "Total Nodes")
	assert.Contains(t, output, "Total Changes")
	assert.Contains(t, output, "Avg Changes/Node")
	assert.Contains(t, output, "Avg Coupling Strength")
}

func TestGenerateText_HottestFunctions(t *testing.T) {
	t.Parallel()

	report := buildTestReport()
	s := NewAnalyzer()

	var buf bytes.Buffer

	err := s.generateText(report, &buf)
	require.NoError(t, err)

	output := buf.String()
	assert.Contains(t, output, "Hottest Functions")
	assert.Contains(t, output, "processPayment")
	assert.Contains(t, output, "changes")
}

func TestGenerateText_StrongestCouplings(t *testing.T) {
	t.Parallel()

	report := buildTestReport()
	s := NewAnalyzer()

	var buf bytes.Buffer

	err := s.generateText(report, &buf)
	require.NoError(t, err)

	output := buf.String()
	assert.Contains(t, output, "Strongest Couplings")
	assert.Contains(t, output, "↔")
	assert.Contains(t, output, "co-changes")
}

func TestGenerateText_EmptyReport(t *testing.T) {
	t.Parallel()

	report := analyze.Report{}
	s := NewAnalyzer()

	var buf bytes.Buffer

	err := s.generateText(report, &buf)
	require.NoError(t, err)

	output := buf.String()
	assert.Contains(t, output, "Shotness Analysis")
	assert.Contains(t, output, "0 nodes")
}

func TestSerialize_Text(t *testing.T) {
	t.Parallel()

	report := buildTestReport()
	s := NewAnalyzer()

	var buf bytes.Buffer

	err := s.Serialize(report, analyze.FormatText, &buf)
	require.NoError(t, err)
	assert.Positive(t, buf.Len())
}

func TestSerialize_JSON_Passthrough(t *testing.T) {
	t.Parallel()

	report := buildTestReport()
	s := NewAnalyzer()

	var buf bytes.Buffer

	err := s.Serialize(report, analyze.FormatJSON, &buf)
	require.NoError(t, err)
	assert.Positive(t, buf.Len())
}

func TestSerialize_UnsupportedFormat(t *testing.T) {
	t.Parallel()

	report := buildTestReport()
	s := NewAnalyzer()

	var buf bytes.Buffer

	err := s.Serialize(report, "invalid_format", &buf)
	require.Error(t, err)
	assert.ErrorIs(t, err, analyze.ErrUnsupportedFormat)
}

func TestFormatNodeLabel_WithFile(t *testing.T) {
	t.Parallel()

	label := formatNodeLabel("processPayment", "pkg/core/engine.go")
	assert.Equal(t, "processPayment (engine.go)", label)
}

func TestFormatNodeLabel_NoFile(t *testing.T) {
	t.Parallel()

	label := formatNodeLabel("processPayment", "")
	assert.Equal(t, "processPayment", label)
}

func TestHotnessColor(t *testing.T) {
	t.Parallel()

	assert.NotEqual(t, hotnessColor(0.0), hotnessColor(1.0))
}

func TestRiskLevelColor(t *testing.T) {
	t.Parallel()

	assert.NotEqual(t, riskLevelColor(RiskLevelHigh), riskLevelColor(RiskLevelLow))
}

func TestCouplingStrengthColor(t *testing.T) {
	t.Parallel()

	assert.NotEqual(t, couplingStrengthColor(0.0), couplingStrengthColor(1.0))
}

func TestGenerateText_RiskAssessment(t *testing.T) {
	t.Parallel()

	report := buildHotReport()
	s := NewAnalyzer()

	var buf bytes.Buffer

	err := s.generateText(report, &buf)
	require.NoError(t, err)

	output := buf.String()
	assert.Contains(t, output, "Risk Assessment")
	assert.Contains(t, output, RiskLevelHigh)
}

func buildTestReport() analyze.Report {
	return analyze.Report{
		"Nodes": []NodeSummary{
			{Type: "Function", Name: "processPayment", File: "pkg/core/engine.go"},
			{Type: "Function", Name: "validateInput", File: "pkg/core/engine.go"},
			{Type: "Function", Name: "handleRequest", File: "pkg/api/handler.go"},
		},
		"Counters": []map[int]int{
			{0: 15, 1: 8, 2: 3},
			{0: 8, 1: 10, 2: 2},
			{0: 3, 1: 2, 2: 5},
		},
	}
}

func buildHotReport() analyze.Report {
	return analyze.Report{
		"Nodes": []NodeSummary{
			{Type: "Function", Name: "hotFunc", File: "hot.go"},
			{Type: "Function", Name: "coldFunc", File: "cold.go"},
		},
		"Counters": []map[int]int{
			{0: HotspotThresholdHigh + 5, 1: 12},
			{0: 12, 1: 3},
		},
	}
}
