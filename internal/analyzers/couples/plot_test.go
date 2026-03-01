package couples

import (
	"bytes"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
)

func TestGeneratePlot_WithData(t *testing.T) {
	t.Parallel()

	c := NewHistoryAnalyzer()

	report := analyze.Report{
		"Files":      []string{"pkg/a.go", "pkg/b.go"},
		"FilesLines": []int{100, 200},
		"FilesMatrix": []map[int]int64{
			{0: 10, 1: 8},
			{0: 8, 1: 12},
		},
		"ReversedPeopleDict": []string{"alice", "bob"},
		"PeopleMatrix": []map[int]int64{
			{0: 20, 1: 10},
			{0: 10, 1: 15},
		},
		"PeopleFiles": [][]int{
			{0, 1},
			{0},
		},
	}

	var buf bytes.Buffer

	err := c.generatePlot(report, &buf)
	require.NoError(t, err)
	assert.NotEmpty(t, buf.String())
	assert.Contains(t, buf.String(), "Couples Analysis")
}

func TestGeneratePlot_EmptyMatrix(t *testing.T) {
	t.Parallel()

	c := NewHistoryAnalyzer()

	report := analyze.Report{
		"Files":              []string{},
		"FilesLines":         []int{},
		"FilesMatrix":        []map[int]int64{},
		"ReversedPeopleDict": []string{},
		"PeopleMatrix":       []map[int]int64{},
		"PeopleFiles":        [][]int{},
	}

	var buf bytes.Buffer

	err := c.generatePlot(report, &buf)
	require.NoError(t, err)
	assert.NotEmpty(t, buf.String())
}

func TestSerializePlotFormat(t *testing.T) {
	t.Parallel()

	c := NewHistoryAnalyzer()

	report := analyze.Report{
		"Files":              []string{"f.go"},
		"FilesLines":         []int{10},
		"FilesMatrix":        []map[int]int64{{0: 5}},
		"ReversedPeopleDict": []string{"dev"},
		"PeopleMatrix":       []map[int]int64{{0: 5}},
		"PeopleFiles":        [][]int{{0}},
	}

	var buf bytes.Buffer

	err := c.Serialize(report, analyze.FormatPlot, &buf)
	require.NoError(t, err)
	assert.NotEmpty(t, buf.String())
}

func TestGenerateSections_WithData(t *testing.T) {
	t.Parallel()

	c := NewHistoryAnalyzer()

	report := analyze.Report{
		"Files":      []string{"a.go", "b.go", "c.go"},
		"FilesLines": []int{100, 200, 50},
		"FilesMatrix": []map[int]int64{
			{0: 10, 1: 8, 2: 3},
			{0: 8, 1: 12, 2: 2},
			{0: 3, 1: 2, 2: 5},
		},
		"ReversedPeopleDict": []string{"alice", "bob"},
		"PeopleMatrix": []map[int]int64{
			{0: 20, 1: 10},
			{0: 10, 1: 15},
		},
		"PeopleFiles": [][]int{
			{0, 1, 2},
			{0, 1},
		},
	}

	sections, err := c.GenerateSections(report)
	require.NoError(t, err)
	// Should have: file coupling bar, developer heatmap, ownership pie.
	require.GreaterOrEqual(t, len(sections), 2) // At least heatmap + one more.

	// Check section titles are present.
	titles := make([]string, len(sections))
	for i, s := range sections {
		titles[i] = s.Title
	}

	assert.Contains(t, titles, "Top File Couples")
	assert.Contains(t, titles, "Developer Coupling Heatmap")
	assert.Contains(t, titles, "File Ownership Distribution")
}

func TestBuildFileCouplingBarChart_NoData(t *testing.T) {
	t.Parallel()

	report := analyze.Report{}
	chart := buildFileCouplingBarChart(report)
	assert.Nil(t, chart)
}

func TestBuildFileCouplingBarChart_WithData(t *testing.T) {
	t.Parallel()

	report := analyze.Report{
		"Files":      []string{"a.go", "b.go"},
		"FilesLines": []int{10, 20},
		"FilesMatrix": []map[int]int64{
			{0: 5, 1: 3},
			{0: 3, 1: 5},
		},
		"ReversedPeopleDict": []string{"dev"},
		"PeopleMatrix":       []map[int]int64{{0: 5}},
		"PeopleFiles":        [][]int{{0, 1}},
	}

	chart := buildFileCouplingBarChart(report)
	assert.NotNil(t, chart)
}

func TestBuildOwnershipPieChart_NoData(t *testing.T) {
	t.Parallel()

	report := analyze.Report{}
	chart := buildOwnershipPieChart(report)
	assert.Nil(t, chart)
}

func TestBuildOwnershipPieChart_WithData(t *testing.T) {
	t.Parallel()

	report := analyze.Report{
		"Files":      []string{"a.go", "b.go", "c.go"},
		"FilesLines": []int{10, 20, 30},
		"FilesMatrix": []map[int]int64{
			{0: 5}, {1: 5}, {2: 5},
		},
		"ReversedPeopleDict": []string{"alice", "bob"},
		"PeopleMatrix": []map[int]int64{
			{0: 10, 1: 5},
			{0: 5, 1: 8},
		},
		"PeopleFiles": [][]int{
			{0, 1},
			{1, 2},
		},
	}

	chart := buildOwnershipPieChart(report)
	assert.NotNil(t, chart)
}

func TestTruncatePath(t *testing.T) {
	t.Parallel()

	// Short path — no truncation.
	assert.Equal(t, "short.go", truncatePath("short.go"))

	// Long path — truncated.
	long := "very/long/path/that/exceeds/thirty/characters/file.go"
	result := truncatePath(long)
	assert.LessOrEqual(t, len(result), 30)
	assert.Equal(t, "...", result[:3])
}

func TestFindMaxValue(t *testing.T) {
	t.Parallel()

	matrix := []map[int]int64{
		{0: 5, 1: 10},
		{0: 3, 1: 7},
	}

	assert.Equal(t, int64(10), findMaxValue(matrix))
}

func TestFindMaxValue_Empty(t *testing.T) {
	t.Parallel()

	assert.Equal(t, int64(0), findMaxValue(nil))
	assert.Equal(t, int64(0), findMaxValue([]map[int]int64{}))
}

func TestBuildHeatMapData(t *testing.T) {
	t.Parallel()

	matrix := []map[int]int64{
		{0: 5, 1: 3},
		{0: 3, 1: 8},
	}
	names := []string{"alice", "bob"}

	data := buildHeatMapData(matrix, names)
	assert.NotEmpty(t, data)
}

func TestExtractCouplesData_DirectExtraction(t *testing.T) {
	t.Parallel()

	report := analyze.Report{
		"PeopleMatrix":       []map[int]int64{{0: 5, 1: 3}, {0: 3, 1: 8}},
		"ReversedPeopleDict": []string{"alice", "bob"},
	}

	matrix, names, err := extractCouplesData(report)
	require.NoError(t, err)
	assert.Len(t, matrix, 2)
	assert.Equal(t, []string{"alice", "bob"}, names)
}

func TestExtractCouplesData_MissingMatrix(t *testing.T) {
	t.Parallel()

	report := analyze.Report{
		"ReversedPeopleDict": []string{"alice", "bob"},
	}

	matrix, names, err := extractCouplesData(report)
	require.Error(t, err)
	assert.Nil(t, matrix)
	assert.Nil(t, names)
}

func TestExtractCouplesData_MissingNames(t *testing.T) {
	t.Parallel()

	report := analyze.Report{
		"PeopleMatrix": []map[int]int64{{0: 5}},
	}

	matrix, names, err := extractCouplesData(report)
	require.Error(t, err)
	assert.Nil(t, matrix)
	assert.Nil(t, names)
}

func TestFindMaxOffDiagonal(t *testing.T) {
	t.Parallel()

	matrix := []map[int]int64{
		{0: 100, 1: 10},
		{0: 10, 1: 200},
	}

	// Diagonal values (100, 200) should be excluded.
	assert.Equal(t, int64(10), findMaxOffDiagonal(matrix))
}

func TestFindMaxOffDiagonal_AllDiagonal(t *testing.T) {
	t.Parallel()

	matrix := []map[int]int64{
		{0: 50},
		{1: 30},
	}

	assert.Equal(t, int64(0), findMaxOffDiagonal(matrix))
}

func TestFindMaxOffDiagonal_Empty(t *testing.T) {
	t.Parallel()

	assert.Equal(t, int64(0), findMaxOffDiagonal(nil))
	assert.Equal(t, int64(0), findMaxOffDiagonal([]map[int]int64{}))
}

func TestShortenDevNames(t *testing.T) {
	t.Parallel()

	names := []string{
		"alice|alice@example.com",
		"bob",
		"a-very-long-developer-name-that-exceeds-limit|long@email.com",
	}

	short := shortenDevNames(names)
	assert.Equal(t, "alice", short[0])
	assert.Equal(t, "bob", short[1])
	assert.LessOrEqual(t, len(short[2]), maxDevNameLen)
	assert.Contains(t, short[2], "...")
}

func TestShortenDevNames_NoPipe(t *testing.T) {
	t.Parallel()

	names := []string{"alice", "bob"}
	short := shortenDevNames(names)
	assert.Equal(t, []string{"alice", "bob"}, short)
}

func TestDynamicHeatmapHeight(t *testing.T) {
	t.Parallel()

	// Small: should clamp to min.
	assert.Equal(t, "400px", dynamicHeatmapHeight(2))

	// Medium: should compute normally.
	assert.Equal(t, "500px", dynamicHeatmapHeight(10)) // 10*30+200=500

	// Large: should clamp to max.
	assert.Equal(t, "900px", dynamicHeatmapHeight(50))
}

func TestRegisterPlotSections(t *testing.T) {
	t.Parallel()

	// Should not panic.
	RegisterPlotSections()
}
