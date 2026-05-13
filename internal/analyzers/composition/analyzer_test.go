package composition

import (
	"bytes"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	filehistory "github.com/Sumatoshi-tech/codefang/internal/analyzers/file_history"
)

func TestAnalyzer_Name(t *testing.T) {
	t.Parallel()

	a := NewAnalyzer()
	assert.Equal(t, analyzerName, a.Name())
}

func TestAnalyzer_Flag(t *testing.T) {
	t.Parallel()

	a := NewAnalyzer()
	assert.Equal(t, analyzerFlag, a.Flag())
}

func TestAnalyzer_Descriptor(t *testing.T) {
	t.Parallel()

	a := NewAnalyzer()
	d := a.Descriptor()
	assert.Equal(t, analyze.ModeStatic, d.Mode)
	assert.Equal(t, analyzerID, d.ID)
}

func TestAnalyzer_Thresholds_Nil(t *testing.T) {
	t.Parallel()

	a := NewAnalyzer()
	assert.Nil(t, a.Thresholds())
}

func TestAnalyzer_AnalyzeContent_GoFile(t *testing.T) {
	t.Parallel()

	a := NewAnalyzer()

	report, err := a.AnalyzeFileContent("pkg/main.go", []byte("package main\n\nfunc main() {}\n"))
	require.NoError(t, err)
	assert.Equal(t, string(filehistory.CategorySource), report[keyCategory])
}

func TestAnalyzer_AnalyzeContent_VendorPath(t *testing.T) {
	t.Parallel()

	a := NewAnalyzer()

	report, err := a.AnalyzeFileContent("vendor/github.com/foo/bar.go", []byte("package bar\n"))
	require.NoError(t, err)
	assert.Equal(t, string(filehistory.CategoryVendor), report[keyCategory])
}

func TestAnalyzer_AnalyzeContent_Markdown(t *testing.T) {
	t.Parallel()

	a := NewAnalyzer()

	report, err := a.AnalyzeFileContent("docs/README.md", []byte("# Hello\n"))
	require.NoError(t, err)
	assert.Equal(t, string(filehistory.CategoryDocumentation), report[keyCategory])
}

func TestAnalyzer_AnalyzeContent_ConfigFile(t *testing.T) {
	t.Parallel()

	a := NewAnalyzer()

	report, err := a.AnalyzeFileContent(".golangci.yml", []byte("linters:\n  enable:\n"))
	require.NoError(t, err)
	assert.Equal(t, string(filehistory.CategoryConfiguration), report[keyCategory])
}

func TestAnalyzer_AnalyzeContent_BinaryContent(t *testing.T) {
	t.Parallel()

	a := NewAnalyzer()

	// Binary content: null bytes trigger enry.IsBinary.
	binary := []byte{0x00, 0x01, 0x02, 0xFF, 0xFE, 0x00, 0x00, 0x00}
	report, err := a.AnalyzeFileContent("data.bin", binary)
	require.NoError(t, err)
	assert.Equal(t, string(filehistory.CategoryBinary), report[keyCategory])
}

func TestAnalyzer_AnalyzeContent_DotFile(t *testing.T) {
	t.Parallel()

	a := NewAnalyzer()

	report, err := a.AnalyzeFileContent(".editorconfig", []byte("[*]\nindent_style = tab\n"))
	require.NoError(t, err)
	assert.Equal(t, string(filehistory.CategoryDotFile), report[keyCategory])
}

func TestAnalyzer_AnalyzeContent_ImagePath(t *testing.T) {
	t.Parallel()

	a := NewAnalyzer()

	report, err := a.AnalyzeFileContent("logo.png", nil)
	require.NoError(t, err)
	assert.Equal(t, string(filehistory.CategoryImage), report[keyCategory])
}

func TestAnalyzer_CreateAggregator(t *testing.T) {
	t.Parallel()

	a := NewAnalyzer()
	agg := a.CreateAggregator()
	require.NotNil(t, agg)
}

func TestAnalyzer_CreateReportSection(t *testing.T) {
	t.Parallel()

	a := NewAnalyzer()
	section := a.CreateReportSection(analyze.Report{})
	require.NotNil(t, section)
	assert.Equal(t, sectionTitle, section.SectionTitle())
}

func TestAnalyzer_ImplementsRawFileAnalyzer(t *testing.T) {
	t.Parallel()

	var _ analyze.RawFileAnalyzer = (*Analyzer)(nil)
}

func TestAnalyzer_FormatReportJSON(t *testing.T) {
	t.Parallel()

	a := NewAnalyzer()

	var buf bytes.Buffer

	err := a.FormatReportJSON(analyze.Report{keyCategory: "source"}, &buf)
	require.NoError(t, err)
	assert.Contains(t, buf.String(), "source")
}

func TestAnalyzer_FormatReportYAML(t *testing.T) {
	t.Parallel()

	a := NewAnalyzer()

	var buf bytes.Buffer

	err := a.FormatReportYAML(analyze.Report{keyCategory: "vendor"}, &buf)
	require.NoError(t, err)
	assert.Contains(t, buf.String(), "vendor")
}

func TestAnalyzer_FormatReport(t *testing.T) {
	t.Parallel()

	a := NewAnalyzer()

	var buf bytes.Buffer

	err := a.FormatReport(analyze.Report{keyCategory: "binary"}, &buf)
	require.NoError(t, err)
	assert.Contains(t, buf.String(), "binary")
}

func TestAnalyzer_FormatReportPlot(t *testing.T) {
	t.Parallel()

	a := NewAnalyzer()

	var buf bytes.Buffer

	err := a.FormatReportPlot(analyze.Report{keyCategory: "docs"}, &buf)
	require.NoError(t, err)
	assert.Contains(t, buf.String(), "docs")
}

func TestAnalyzer_FormatReportBinary(t *testing.T) {
	t.Parallel()

	a := NewAnalyzer()

	var buf bytes.Buffer

	err := a.FormatReportBinary(analyze.Report{keyCategory: "source"}, &buf)
	require.NoError(t, err)
	assert.NotEmpty(t, buf.Bytes())
}

func TestAnalyzer_Configure_NoError(t *testing.T) {
	t.Parallel()

	a := NewAnalyzer()
	assert.NoError(t, a.Configure(nil))
}

func TestAnalyzer_ListConfigurationOptions_Empty(t *testing.T) {
	t.Parallel()

	a := NewAnalyzer()
	assert.Nil(t, a.ListConfigurationOptions())
}

// Aggregator tests.

func TestAggregator_EmptyResult(t *testing.T) {
	t.Parallel()

	agg := NewAggregator()
	result := agg.GetResult()

	total, ok := result[keyTotalFiles].(int)
	require.True(t, ok)
	assert.Equal(t, 0, total)
}

func TestAggregator_SingleFile(t *testing.T) {
	t.Parallel()

	agg := NewAggregator()
	agg.Aggregate(map[string]analyze.Report{
		analyzerName: {keyCategory: string(filehistory.CategorySource)},
	})

	result := agg.GetResult()

	total, ok := result[keyTotalFiles].(int)
	require.True(t, ok)
	assert.Equal(t, 1, total)

	breakdown, ok := result[keyBreakdown].(map[string]int)
	require.True(t, ok)
	assert.Equal(t, 1, breakdown[string(filehistory.CategorySource)])
}

func TestAggregator_MultipleFiles(t *testing.T) {
	t.Parallel()

	agg := NewAggregator()

	// 3 source + 1 vendor + 1 docs = 5 total.
	files := []filehistory.Category{
		filehistory.CategorySource,
		filehistory.CategorySource,
		filehistory.CategorySource,
		filehistory.CategoryVendor,
		filehistory.CategoryDocumentation,
	}

	for _, cat := range files {
		agg.Aggregate(map[string]analyze.Report{
			analyzerName: {keyCategory: string(cat)},
		})
	}

	result := agg.GetResult()

	total, ok := result[keyTotalFiles].(int)
	require.True(t, ok)
	assert.Equal(t, len(files), total)

	breakdown, ok := result[keyBreakdown].(map[string]int)
	require.True(t, ok)
	assert.Equal(t, 3, breakdown[string(filehistory.CategorySource)])
	assert.Equal(t, 1, breakdown[string(filehistory.CategoryVendor)])
	assert.Equal(t, 1, breakdown[string(filehistory.CategoryDocumentation)])

	percentages, ok := result[keyPercentage].(map[string]float64)
	require.True(t, ok)
	assert.InDelta(t, 60.0, percentages[string(filehistory.CategorySource)], 0.1)
	assert.InDelta(t, 20.0, percentages[string(filehistory.CategoryVendor)], 0.1)
	assert.InDelta(t, 20.0, percentages[string(filehistory.CategoryDocumentation)], 0.1)
}

func TestAggregator_SkipsInvalidCategory(t *testing.T) {
	t.Parallel()

	agg := NewAggregator()
	agg.Aggregate(map[string]analyze.Report{
		analyzerName: {"not_a_category": 42},
	})

	result := agg.GetResult()

	total, ok := result[keyTotalFiles].(int)
	require.True(t, ok)
	// File counted but no category incremented.
	assert.Equal(t, 1, total)
}
