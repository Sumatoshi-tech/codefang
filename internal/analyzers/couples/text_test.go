package couples

import (
	"bytes"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/common/terminal"
)

func TestGenerateText_EmptyReport(t *testing.T) {
	t.Parallel()

	c := NewHistoryAnalyzer()

	var buf bytes.Buffer

	err := c.generateText(analyze.Report{}, &buf)
	require.NoError(t, err)

	output := buf.String()
	assert.Contains(t, output, "Couples")
	assert.Contains(t, output, "0 files")
}

func TestGenerateText_WithData(t *testing.T) {
	t.Parallel()

	c := NewHistoryAnalyzer()

	report := analyze.Report{
		"Files":              []string{"pkg/a.go", "pkg/b.go", "pkg/c.go"},
		"FilesLines":         []int{100, 200, 50},
		"ReversedPeopleDict": []string{"alice", "bob"},
		"FilesMatrix": []map[int]int64{
			{0: 10, 1: 8, 2: 3},
			{0: 8, 1: 12, 2: 2},
			{0: 3, 1: 2, 2: 5},
		},
		"PeopleMatrix": []map[int]int64{
			{0: 20, 1: 10},
			{0: 10, 1: 15},
		},
		"PeopleFiles": [][]int{
			{0, 1, 2},
			{0, 1},
		},
	}

	var buf bytes.Buffer

	err := c.generateText(report, &buf)
	require.NoError(t, err)

	output := buf.String()
	// Header.
	assert.Contains(t, output, "Couples")
	assert.Contains(t, output, "3 files")
	// Summary.
	assert.Contains(t, output, "Summary")
	assert.Contains(t, output, "Total Files")
	assert.Contains(t, output, "Total Developers")
	// File couples.
	assert.Contains(t, output, "Top File Couples")
	assert.Contains(t, output, "pkg/a.go")
	assert.Contains(t, output, "pkg/b.go")
	// Developer couples.
	assert.Contains(t, output, "Top Developer Couples")
	assert.Contains(t, output, "alice")
	assert.Contains(t, output, "bob")
	// Ownership.
	assert.Contains(t, output, "File Ownership Risk")
}

func TestGenerateText_Serialize_TextFormat(t *testing.T) {
	t.Parallel()

	c := NewHistoryAnalyzer()

	report := analyze.Report{
		"Files":              []string{"f1.go"},
		"FilesLines":         []int{10},
		"ReversedPeopleDict": []string{"dev"},
		"FilesMatrix":        []map[int]int64{{0: 5}},
		"PeopleMatrix":       []map[int]int64{{0: 10}},
		"PeopleFiles":        [][]int{{0}},
	}

	var buf bytes.Buffer

	err := c.Serialize(report, analyze.FormatText, &buf)
	require.NoError(t, err)
	assert.Contains(t, buf.String(), "Couples")
}

func TestSortOwnershipByRisk(t *testing.T) {
	t.Parallel()

	ownership := []FileOwnershipData{
		{File: "a.go", Contributors: 3},
		{File: "b.go", Contributors: 1},
		{File: "c.go", Contributors: 2},
	}

	sorted := SortOwnershipByRisk(ownership)

	require.Len(t, sorted, 3)
	assert.Equal(t, "b.go", sorted[0].File) // 1 contributor = highest risk.
	assert.Equal(t, "c.go", sorted[1].File)
	assert.Equal(t, "a.go", sorted[2].File) // 3 contributors = lowest risk.
}

func TestColorForStrength(t *testing.T) {
	t.Parallel()

	assert.Equal(t, terminal.ColorRed, colorForStrength(0.8))
	assert.Equal(t, terminal.ColorYellow, colorForStrength(0.5))
	assert.Equal(t, terminal.ColorGreen, colorForStrength(0.2))
}

func TestFormatPct(t *testing.T) {
	t.Parallel()

	assert.Equal(t, "50%", formatPct(0.5))
	assert.Equal(t, "100%", formatPct(1.0))
	assert.Equal(t, "0%", formatPct(0.0))
}
