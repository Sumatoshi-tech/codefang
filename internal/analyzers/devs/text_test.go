package devs

import (
	"bytes"
	"testing"
	"time"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/common/terminal"
	pkgplumbing "github.com/Sumatoshi-tech/codefang/internal/plumbing"
)

func TestGenerateText_ValidReport(t *testing.T) {
	t.Parallel()

	ticks := map[int]map[int]*DevTick{
		0: {
			0: {
				LineStats: pkgplumbing.LineStats{Added: 500, Removed: 100, Changed: 50},
				Languages: map[string]pkgplumbing.LineStats{
					"Go":     {Added: 400, Removed: 80, Changed: 40},
					"Python": {Added: 100, Removed: 20, Changed: 10},
				},
				Commits: 15,
			},
			1: {
				LineStats: pkgplumbing.LineStats{Added: 200, Removed: 50, Changed: 20},
				Languages: map[string]pkgplumbing.LineStats{
					"Go": {Added: 200, Removed: 50, Changed: 20},
				},
				Commits: 8,
			},
		},
		1: {
			0: {
				LineStats: pkgplumbing.LineStats{Added: 300, Removed: 60, Changed: 30},
				Languages: map[string]pkgplumbing.LineStats{
					"Go": {Added: 300, Removed: 60, Changed: 30},
				},
				Commits: 10,
			},
		},
	}
	report := ticksToCanonicalReport(ticks, []string{"Alice", "Bob"})

	a := NewAnalyzer()

	var buf bytes.Buffer

	err := a.generateText(report, &buf)
	require.NoError(t, err)

	output := buf.String()
	assert.Contains(t, output, "Developers")
	assert.Contains(t, output, "Summary")
	assert.Contains(t, output, "Total Commits")
	assert.Contains(t, output, "Developers")
	assert.Contains(t, output, "Active Developers")
	assert.Contains(t, output, "Project Bus Factor")
	assert.Contains(t, output, "Top Contributors")
	assert.Contains(t, output, "Alice")
	assert.Contains(t, output, "Bob")
	assert.Contains(t, output, "Bus Factor Risk")
	assert.Contains(t, output, "Churn Summary")
	assert.Contains(t, output, "Lines Added")
	assert.Contains(t, output, "Lines Removed")
	assert.Contains(t, output, "Net Change")
}

func TestGenerateText_EmptyReport(t *testing.T) {
	t.Parallel()

	report := analyze.Report{
		"CommitDevData":      map[string]*CommitDevData{},
		"CommitsByTick":      map[int][]any{},
		"ReversedPeopleDict": []string{},
		"TickSize":           24 * time.Hour,
	}

	a := NewAnalyzer()

	var buf bytes.Buffer

	err := a.generateText(report, &buf)
	require.NoError(t, err)

	output := buf.String()
	assert.Contains(t, output, "Developers")
	assert.Contains(t, output, "Summary")
	// No contributors or bus factor sections for empty data.
	assert.NotContains(t, output, "Top Contributors")
	assert.NotContains(t, output, "Bus Factor Risk")
}

func TestSerialize_Text(t *testing.T) {
	t.Parallel()

	ticks := map[int]map[int]*DevTick{
		0: {
			0: {
				LineStats: pkgplumbing.LineStats{Added: 100, Removed: 20},
				Languages: map[string]pkgplumbing.LineStats{
					"Go": {Added: 100, Removed: 20},
				},
				Commits: 5,
			},
		},
	}
	report := ticksToCanonicalReport(ticks, []string{"Alice"})

	a := NewAnalyzer()

	var buf bytes.Buffer

	err := a.Serialize(report, analyze.FormatText, &buf)
	require.NoError(t, err)
	assert.Positive(t, buf.Len())
	assert.Contains(t, buf.String(), "Alice")
}

func TestSerialize_JSON(t *testing.T) {
	t.Parallel()

	ticks := map[int]map[int]*DevTick{
		0: {
			0: {
				LineStats: pkgplumbing.LineStats{Added: 100, Removed: 20},
				Commits:   5,
			},
		},
	}
	report := ticksToCanonicalReport(ticks, []string{"Alice"})

	a := NewAnalyzer()

	var buf bytes.Buffer

	err := a.Serialize(report, analyze.FormatJSON, &buf)
	require.NoError(t, err)
	assert.Contains(t, buf.String(), "Alice")
	assert.Contains(t, buf.String(), "developers")
}

func TestSerialize_Plot(t *testing.T) {
	t.Parallel()

	ticks := map[int]map[int]*DevTick{
		0: {
			0: {
				LineStats: pkgplumbing.LineStats{Added: 100, Removed: 20},
				Languages: map[string]pkgplumbing.LineStats{
					"Go": {Added: 100, Removed: 20},
				},
				Commits: 5,
			},
		},
	}
	report := ticksToCanonicalReport(ticks, []string{"Alice"})

	a := NewAnalyzer()

	var buf bytes.Buffer

	err := a.Serialize(report, analyze.FormatPlot, &buf)
	require.NoError(t, err)
	assert.Contains(t, buf.String(), "Developer Analytics Dashboard")
}

func TestGenerateText_BusFactorRiskColors(t *testing.T) {
	t.Parallel()

	// Create a scenario with one dominant dev to trigger CRITICAL risk.
	ticks := map[int]map[int]*DevTick{
		0: {
			0: {
				LineStats: pkgplumbing.LineStats{Added: 950, Removed: 50},
				Languages: map[string]pkgplumbing.LineStats{
					"Go": {Added: 950, Removed: 50},
				},
				Commits: 20,
			},
			1: {
				LineStats: pkgplumbing.LineStats{Added: 50, Removed: 5},
				Languages: map[string]pkgplumbing.LineStats{
					"Go": {Added: 50, Removed: 5},
				},
				Commits: 2,
			},
		},
	}
	report := ticksToCanonicalReport(ticks, []string{"Hero", "Minor"})

	a := NewAnalyzer()

	var buf bytes.Buffer

	err := a.generateText(report, &buf)
	require.NoError(t, err)

	output := buf.String()
	assert.Contains(t, output, "CRITICAL")
	assert.Contains(t, output, "Go")
}

func TestFormatInt(t *testing.T) {
	t.Parallel()

	assert.Equal(t, "0", formatInt(0))
	assert.Equal(t, "999", formatInt(999))
	assert.Equal(t, "1,000", formatInt(1000))
	assert.Equal(t, "52,340", formatInt(52340))
	assert.Equal(t, "1,234,567", formatInt(1234567))
	assert.Equal(t, "-100", formatInt(-100))
}

func TestRiskToColor(t *testing.T) {
	t.Parallel()

	tests := []struct {
		level string
		want  terminal.Color
	}{
		{RiskCritical, terminal.ColorRed},
		{RiskHigh, terminal.ColorRed},
		{RiskMedium, terminal.ColorYellow},
		{RiskLow, terminal.ColorGreen},
	}

	for _, tt := range tests {
		got := riskToColor(tt.level)
		assert.Equal(t, tt.want, got, "riskToColor(%s)", tt.level)
	}
}
