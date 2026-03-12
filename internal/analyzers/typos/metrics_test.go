package typos

import (
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

// Test constants to avoid magic strings/numbers.
const (
	testWrong1   = "getUsrNam"
	testCorrect1 = "getUserName"
	testWrong2   = "calcTotl"
	testCorrect2 = "calcTotal"
	testFile1    = "file1.go"
	testFile2    = "file2.go"
	testLine1    = 10
	testLine2    = 20
)

// Helper function to create test hash.
func testHash(s string) gitlib.Hash {
	var h gitlib.Hash
	copy(h[:], s)

	return h
}

// --- ParseReportData Tests ---.

func TestParseReportData_Empty(t *testing.T) {
	t.Parallel()

	report := analyze.Report{}

	result, err := ParseReportData(report)

	require.NoError(t, err)
	assert.Empty(t, result.Typos)
}

func TestParseReportData_WithTypos(t *testing.T) {
	t.Parallel()

	typos := []Typo{
		{Wrong: testWrong1, Correct: testCorrect1, File: testFile1, Line: testLine1, Commit: testHash("abc123")},
		{Wrong: testWrong2, Correct: testCorrect2, File: testFile2, Line: testLine2, Commit: testHash("def456")},
	}

	report := analyze.Report{
		"typos": typos,
	}

	result, err := ParseReportData(report)

	require.NoError(t, err)
	require.Len(t, result.Typos, 2)

	assert.Equal(t, testWrong1, result.Typos[0].Wrong)
	assert.Equal(t, testCorrect1, result.Typos[0].Correct)
	assert.Equal(t, testFile1, result.Typos[0].File)
	assert.Equal(t, testLine1, result.Typos[0].Line)
}

// --- TypoListMetric Tests ---.

func TestTypoListMetric_Empty(t *testing.T) {
	t.Parallel()

	input := &ReportData{}
	result := computeTypoList(input)

	assert.Empty(t, result)
}

func TestTypoListMetric_SingleTypo(t *testing.T) {
	t.Parallel()

	hash := testHash("abc12345")
	input := &ReportData{
		Typos: []Typo{
			{Wrong: testWrong1, Correct: testCorrect1, File: testFile1, Line: testLine1, Commit: hash},
		},
	}

	result := computeTypoList(input)

	require.Len(t, result, 1)
	assert.Equal(t, testWrong1, result[0].Wrong)
	assert.Equal(t, testCorrect1, result[0].Correct)
	assert.Equal(t, testFile1, result[0].File)
	assert.Equal(t, testLine1, result[0].Line)
	assert.Equal(t, hash.String(), result[0].Commit)
}

func TestTypoListMetric_MultipleTypos(t *testing.T) {
	t.Parallel()

	input := &ReportData{
		Typos: []Typo{
			{Wrong: testWrong1, Correct: testCorrect1, File: testFile1, Line: testLine1, Commit: testHash("abc")},
			{Wrong: testWrong2, Correct: testCorrect2, File: testFile2, Line: testLine2, Commit: testHash("def")},
		},
	}

	result := computeTypoList(input)

	require.Len(t, result, 2)
	assert.Equal(t, testWrong1, result[0].Wrong)
	assert.Equal(t, testWrong2, result[1].Wrong)
}

// --- TypoPatternMetric Tests ---.

func TestTypoPatternMetric_Empty(t *testing.T) {
	t.Parallel()

	input := &ReportData{}

	result := computeTypoPatterns(input)

	assert.Empty(t, result)
}

func TestTypoPatternMetric_SingleOccurrence_Excluded(t *testing.T) {
	t.Parallel()

	input := &ReportData{
		Typos: []Typo{
			{Wrong: testWrong1, Correct: testCorrect1},
		},
	}

	result := computeTypoPatterns(input)

	// Single occurrence is excluded (only freq > 1).
	assert.Empty(t, result)
}

func TestTypoPatternMetric_MultipleOccurrences(t *testing.T) {
	t.Parallel()

	input := &ReportData{
		Typos: []Typo{
			{Wrong: testWrong1, Correct: testCorrect1, File: testFile1},
			{Wrong: testWrong1, Correct: testCorrect1, File: testFile2}, // Same pattern.
			{Wrong: testWrong2, Correct: testCorrect2, File: testFile1}, // Different pattern, once.
		},
	}

	result := computeTypoPatterns(input)

	require.Len(t, result, 1) // Only the repeated pattern.
	assert.Equal(t, testWrong1, result[0].Wrong)
	assert.Equal(t, testCorrect1, result[0].Correct)
	assert.Equal(t, 2, result[0].Frequency)
}

func TestTypoPatternMetric_SortedByFrequency(t *testing.T) {
	t.Parallel()

	input := &ReportData{
		Typos: []Typo{
			// Pattern 1: 2 occurrences.
			{Wrong: testWrong1, Correct: testCorrect1},
			{Wrong: testWrong1, Correct: testCorrect1},
			// Pattern 2: 3 occurrences.
			{Wrong: testWrong2, Correct: testCorrect2},
			{Wrong: testWrong2, Correct: testCorrect2},
			{Wrong: testWrong2, Correct: testCorrect2},
		},
	}

	result := computeTypoPatterns(input)

	require.Len(t, result, 2)
	// Sorted by frequency descending.
	assert.Equal(t, testWrong2, result[0].Wrong)
	assert.Equal(t, 3, result[0].Frequency)
	assert.Equal(t, testWrong1, result[1].Wrong)
	assert.Equal(t, 2, result[1].Frequency)
}

// --- FileTypoMetric Tests ---.

func TestFileTypoMetric_Empty(t *testing.T) {
	t.Parallel()

	input := &ReportData{}

	result := computeFileTypos(input)

	assert.Empty(t, result)
}

func TestFileTypoMetric_SingleFile(t *testing.T) {
	t.Parallel()

	input := &ReportData{
		Typos: []Typo{
			{Wrong: testWrong1, Correct: testCorrect1, File: testFile1},
			{Wrong: testWrong2, Correct: testCorrect2, File: testFile1},
		},
	}

	result := computeFileTypos(input)

	require.Len(t, result, 1)
	assert.Equal(t, testFile1, result[0].File)
	assert.Equal(t, 2, result[0].TypoCount)
	assert.Equal(t, 2, result[0].FixedTypos)
}

func TestFileTypoMetric_MultipleFiles_SortedByCount(t *testing.T) {
	t.Parallel()

	input := &ReportData{
		Typos: []Typo{
			{Wrong: testWrong1, Correct: testCorrect1, File: testFile1},
			{Wrong: testWrong2, Correct: testCorrect2, File: testFile2},
			{Wrong: "foo", Correct: "bar", File: testFile2},
			{Wrong: "baz", Correct: "qux", File: testFile2},
		},
	}

	result := computeFileTypos(input)

	require.Len(t, result, 2)
	// Sorted by typo count descending.
	assert.Equal(t, testFile2, result[0].File)
	assert.Equal(t, 3, result[0].TypoCount)
	assert.Equal(t, testFile1, result[1].File)
	assert.Equal(t, 1, result[1].TypoCount)
}

// --- TyposAggregateMetric Tests ---.

func TestTyposAggregateMetric_Empty(t *testing.T) {
	t.Parallel()

	input := &ReportData{}

	result := computeAggregate(input)

	assert.Equal(t, 0, result.TotalTypos)
	assert.Equal(t, 0, result.UniquePatterns)
	assert.Equal(t, 0, result.AffectedFiles)
	assert.Equal(t, 0, result.AffectedCommits)
}

func TestTyposAggregateMetric_WithData(t *testing.T) {
	t.Parallel()

	hash1 := testHash("abc")
	hash2 := testHash("def")
	input := &ReportData{
		Typos: []Typo{
			{Wrong: testWrong1, Correct: testCorrect1, File: testFile1, Commit: hash1},
			{Wrong: testWrong1, Correct: testCorrect1, File: testFile2, Commit: hash1}, // Same pattern, different file, same commit.
			{Wrong: testWrong2, Correct: testCorrect2, File: testFile1, Commit: hash2}, // Different pattern, same file, different commit.
		},
	}

	result := computeAggregate(input)

	assert.Equal(t, 3, result.TotalTypos)
	assert.Equal(t, 2, result.UniquePatterns)  // 2 unique patterns.
	assert.Equal(t, 2, result.AffectedFiles)   // 2 unique files.
	assert.Equal(t, 2, result.AffectedCommits) // 2 unique commits.
}

// --- ComputeAllMetrics Tests ---.

func TestComputeAllMetrics_Empty(t *testing.T) {
	t.Parallel()

	report := analyze.Report{}

	ms, err := ComputeAllMetrics(report)

	require.NoError(t, err)

	metrics := ms.Metrics()
	require.Len(t, metrics, 4)

	// All metric values should be empty/zero.
	typoList, ok := metrics[0].Value.([]TypoData)
	require.True(t, ok)
	assert.Empty(t, typoList)

	patterns, ok := metrics[1].Value.([]TypoPatternData)
	require.True(t, ok)
	assert.Empty(t, patterns)

	fileTypos, ok := metrics[2].Value.([]FileTypoData)
	require.True(t, ok)
	assert.Empty(t, fileTypos)

	agg, ok := metrics[3].Value.(AggregateData)
	require.True(t, ok)
	assert.Equal(t, 0, agg.TotalTypos)
}

func TestComputeAllMetrics_Full(t *testing.T) {
	t.Parallel()

	const testLine3 = 30

	typos := []Typo{
		{Wrong: testWrong1, Correct: testCorrect1, File: testFile1, Line: testLine1, Commit: testHash("abc")},
		{Wrong: testWrong1, Correct: testCorrect1, File: testFile2, Line: testLine2, Commit: testHash("def")},
		{Wrong: testWrong2, Correct: testCorrect2, File: testFile1, Line: testLine3, Commit: testHash("ghi")},
	}

	report := analyze.Report{
		"typos": typos,
	}

	ms, err := ComputeAllMetrics(report)
	require.NoError(t, err)

	// Access via ToJSON map for backward-compatible key check.
	jsonMap, ok := ms.ToJSON().(map[string]any)
	require.True(t, ok)

	// TypoList.
	typoList, ok := jsonMap[metricNameTypoList].([]TypoData)
	require.True(t, ok)
	require.Len(t, typoList, 3)

	// Patterns — only testWrong1 has freq > 1.
	patternList, ok := jsonMap[metricNamePatterns].([]TypoPatternData)
	require.True(t, ok)
	require.Len(t, patternList, 1)
	assert.Equal(t, testWrong1, patternList[0].Wrong)
	assert.Equal(t, 2, patternList[0].Frequency)

	// FileTypos.
	fileTypoList, ok := jsonMap[metricNameFileTypos].([]FileTypoData)
	require.True(t, ok)
	require.Len(t, fileTypoList, 2)

	// Aggregate.
	agg, ok := jsonMap[metricNameAggregate].(AggregateData)
	require.True(t, ok)
	assert.Equal(t, 3, agg.TotalTypos)
	assert.Equal(t, 2, agg.UniquePatterns)
	assert.Equal(t, 2, agg.AffectedFiles)
	assert.Equal(t, 3, agg.AffectedCommits)
}

// --- MetricSet Interface Tests ---.

func TestComputeAllMetrics_AnalyzerName(t *testing.T) {
	t.Parallel()

	ms, err := ComputeAllMetrics(analyze.Report{})
	require.NoError(t, err)

	assert.Equal(t, analyzerNameTypos, ms.AnalyzerName())
}

func TestComputeAllMetrics_ToJSON_KeysMatchMetricNames(t *testing.T) {
	t.Parallel()

	ms, err := ComputeAllMetrics(analyze.Report{})
	require.NoError(t, err)

	jsonMap, ok := ms.ToJSON().(map[string]any)
	require.True(t, ok)

	assert.Contains(t, jsonMap, metricNameTypoList)
	assert.Contains(t, jsonMap, metricNamePatterns)
	assert.Contains(t, jsonMap, metricNameFileTypos)
	assert.Contains(t, jsonMap, metricNameAggregate)
}

func TestComputeAllMetrics_ToYAML_MatchesToJSON(t *testing.T) {
	t.Parallel()

	ms, err := ComputeAllMetrics(analyze.Report{})
	require.NoError(t, err)

	assert.Equal(t, ms.ToJSON(), ms.ToYAML())
}
