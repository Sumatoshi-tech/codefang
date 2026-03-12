package burndown

import (
	"bytes"
	"testing"
	"time"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/common/terminal"
)

func TestGenerateText_ValidReport(t *testing.T) {
	t.Parallel()

	report := analyze.Report{
		"GlobalHistory": DenseHistory{
			{100, 50, 25},
			{120, 60, 30},
		},
		"Sampling":    30,
		"Granularity": 30,
		"TickSize":    24 * time.Hour,
		"ProjectName": "myrepo",
	}

	b := NewHistoryAnalyzer()

	var buf bytes.Buffer

	err := b.generateText(report, &buf)
	require.NoError(t, err)

	output := buf.String()
	assert.Contains(t, output, "Burndown: myrepo")
	assert.Contains(t, output, "Summary")
	assert.Contains(t, output, "Current Lines")
	assert.Contains(t, output, "Peak Lines")
	assert.Contains(t, output, "Survival Rate")
}

func TestGenerateText_WithDevelopers(t *testing.T) {
	t.Parallel()

	report := analyze.Report{
		"GlobalHistory":      DenseHistory{{100, 200}, {80, 150}},
		"PeopleHistories":    []DenseHistory{{{100, 200}}, {{50, 100}}},
		"ReversedPeopleDict": []string{"Alice", "Bob"},
		"Sampling":           30,
		"Granularity":        30,
		"TickSize":           24 * time.Hour,
		"ProjectName":        "devrepo",
	}

	b := NewHistoryAnalyzer()

	var buf bytes.Buffer

	err := b.generateText(report, &buf)
	require.NoError(t, err)

	output := buf.String()
	assert.Contains(t, output, "Top Developers")
	assert.Contains(t, output, "Alice")
}

func TestGenerateText_EmptyReport(t *testing.T) {
	t.Parallel()

	report := analyze.Report{}

	b := NewHistoryAnalyzer()

	var buf bytes.Buffer

	err := b.generateText(report, &buf)
	require.NoError(t, err)

	output := buf.String()
	assert.Contains(t, output, "Burndown: project")
	assert.Contains(t, output, "0d")
}

func TestSerialize_Text(t *testing.T) {
	t.Parallel()

	report := analyze.Report{
		"GlobalHistory": DenseHistory{{100, 50}, {120, 60}},
		"Sampling":      30,
		"Granularity":   30,
		"TickSize":      24 * time.Hour,
	}

	b := NewHistoryAnalyzer()

	var buf bytes.Buffer

	err := b.Serialize(report, analyze.FormatText, &buf)
	require.NoError(t, err)
	assert.Positive(t, buf.Len())
}

func TestBuildAgeBands_Empty(t *testing.T) {
	t.Parallel()

	bands := buildAgeBands(nil, 0)
	assert.Empty(t, bands)
}

func TestBuildAgeBands_SingleBand(t *testing.T) {
	t.Parallel()

	bands := buildAgeBands([]int64{500}, 1)
	require.NotEmpty(t, bands)
	assert.Equal(t, "< 1 month", bands[0].label)
	assert.Equal(t, int64(500), bands[0].lines)
}

func TestBuildAgeBands_MultipleBands(t *testing.T) {
	t.Parallel()

	// 24 bands: 0-11 are <12 months, 12-23 are >12 months.
	breakdown := make([]int64, 24)
	for i := range 24 {
		breakdown[i] = 100
	}

	bands := buildAgeBands(breakdown, 24)
	require.NotEmpty(t, bands)

	// Should have aggregated into age buckets.
	var totalLines int64
	for _, b := range bands {
		totalLines += b.lines
	}

	assert.Equal(t, int64(2400), totalLines)
}

func TestFormatInt64(t *testing.T) {
	t.Parallel()

	assert.Equal(t, "0", formatInt64(0))
	assert.Equal(t, "999", formatInt64(999))
	assert.Equal(t, "1,000", formatInt64(1000))
	assert.Equal(t, "52,340", formatInt64(52340))
	assert.Equal(t, "1,234,567", formatInt64(1234567))
	assert.Equal(t, "-100", formatInt64(-100))
}

func TestWriteTopDevelopers_MoreThanMax(t *testing.T) {
	t.Parallel()

	devs := make([]DeveloperSurvivalData, 8)
	for i := range devs {
		devs[i] = DeveloperSurvivalData{
			ID:           i,
			Name:         "",
			CurrentLines: int64(100 - i*10),
			PeakLines:    200,
			SurvivalRate: 0.5,
		}
	}

	metrics := &ComputedMetrics{
		DeveloperSurvival: devs,
	}

	var buf bytes.Buffer

	cfg := terminal.Config{Width: 80, NoColor: true}
	writeTopDevelopers(&buf, cfg, metrics)

	output := buf.String()
	assert.Contains(t, output, "and 3 more...")
}

func TestGenerateText_AgeDistribution(t *testing.T) {
	t.Parallel()

	// Create a report with 6 bands to test multiple age bucket aggregation.
	report := analyze.Report{
		"GlobalHistory": DenseHistory{
			{100, 80, 60, 40, 20, 10},
		},
		"Sampling":    30,
		"Granularity": 30,
		"TickSize":    24 * time.Hour,
		"ProjectName": "agerepo",
	}

	b := NewHistoryAnalyzer()

	var buf bytes.Buffer

	err := b.generateText(report, &buf)
	require.NoError(t, err)

	output := buf.String()
	assert.Contains(t, output, "Code Age Distribution")
}
