package couples

import (
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
)

// Test constants to avoid magic strings/numbers.
const (
	testFile1 = "file1.go"
	testFile2 = "file2.go"
	testFile3 = "file3.go"
	testDev1  = "developer1"
	testDev2  = "developer2"
	testDev3  = "developer3"

	floatDelta = 0.01
)

// --- ParseReportData Tests ---.

func TestParseReportData_Empty(t *testing.T) {
	t.Parallel()

	report := analyze.Report{}

	result, err := ParseReportData(report)

	require.NoError(t, err)
	assert.Empty(t, result.PeopleMatrix)
	assert.Empty(t, result.PeopleFiles)
	assert.Empty(t, result.Files)
	assert.Empty(t, result.FilesLines)
	assert.Empty(t, result.FilesMatrix)
	assert.Empty(t, result.ReversedPeopleDict)
}

func TestParseReportData_AllFields(t *testing.T) {
	t.Parallel()

	report := analyze.Report{
		"PeopleMatrix":       []map[int]int64{{0: 10, 1: 5}, {0: 5, 1: 8}},
		"PeopleFiles":        [][]int{{0, 1}, {1, 2}},
		"Files":              []string{testFile1, testFile2, testFile3},
		"FilesLines":         []int{100, 200, 150},
		"FilesMatrix":        []map[int]int64{{0: 5, 1: 3}, {0: 3, 1: 4}},
		"ReversedPeopleDict": []string{testDev1, testDev2},
	}

	result, err := ParseReportData(report)

	require.NoError(t, err)
	require.Len(t, result.PeopleMatrix, 2)
	require.Len(t, result.PeopleFiles, 2)
	require.Len(t, result.Files, 3)
	require.Len(t, result.FilesLines, 3)
	require.Len(t, result.FilesMatrix, 2)
	require.Len(t, result.ReversedPeopleDict, 2)

	assert.Equal(t, []string{testFile1, testFile2, testFile3}, result.Files)
	assert.Equal(t, []int{100, 200, 150}, result.FilesLines)
	assert.Equal(t, []string{testDev1, testDev2}, result.ReversedPeopleDict)
}

// --- FileCouplingMetric Tests ---.

func TestNewFileCouplingMetric_Metadata(t *testing.T) {
	t.Parallel()

	m := NewFileCouplingMetric()

	assert.Equal(t, "file_coupling", m.Name())
	assert.Equal(t, "File Coupling", m.DisplayName())
	assert.Contains(t, m.Description(), "pairs of files change together")
	assert.Equal(t, "list", m.Type())
}

func TestFileCouplingMetric_Empty(t *testing.T) {
	t.Parallel()

	m := NewFileCouplingMetric()
	input := &ReportData{}

	result := m.Compute(input)

	assert.Empty(t, result)
}

func TestFileCouplingMetric_SinglePair(t *testing.T) {
	t.Parallel()

	m := NewFileCouplingMetric()
	input := &ReportData{
		Files: []string{testFile1, testFile2},
		FilesMatrix: []map[int]int64{
			{0: 10, 1: 5}, // file1 self=10, coupled with file2=5.
			{0: 5, 1: 8},  // file2 self=8, coupled with file1=5.
		},
	}

	result := m.Compute(input)

	require.Len(t, result, 1)
	assert.Equal(t, testFile1, result[0].File1)
	assert.Equal(t, testFile2, result[0].File2)
	assert.Equal(t, int64(5), result[0].CoChanges)
	// Strength = co_changes / avg(self_file1, self_file2) = 5 / avg(10, 8) = 5/9 ≈ 0.556.
	assert.InDelta(t, 5.0/9.0, result[0].Strength, floatDelta)
}

func TestFileCouplingMetric_MultiplePairs_SortedByCoChanges(t *testing.T) {
	t.Parallel()

	m := NewFileCouplingMetric()
	input := &ReportData{
		Files: []string{testFile1, testFile2, testFile3},
		FilesMatrix: []map[int]int64{
			{0: 10, 1: 3, 2: 8}, // file1.
			{0: 3, 1: 5, 2: 2},  // file2.
			{0: 8, 1: 2, 2: 6},  // file3.
		},
	}

	result := m.Compute(input)

	require.Len(t, result, 3) // 3 unique pairs.
	// Should be sorted by CoChanges descending.
	assert.Equal(t, int64(8), result[0].CoChanges) // file1-file3.
	assert.Equal(t, int64(3), result[1].CoChanges) // file1-file2.
	assert.Equal(t, int64(2), result[2].CoChanges) // file2-file3.
}

func TestFileCouplingMetric_SkipsZeroCoChanges(t *testing.T) {
	t.Parallel()

	m := NewFileCouplingMetric()
	input := &ReportData{
		Files: []string{testFile1, testFile2, testFile3},
		FilesMatrix: []map[int]int64{
			{0: 10, 1: 5}, // file1-file2 = 5, file1-file3 = 0 (not present).
			{0: 5, 1: 8},  // file2.
			{0: 0, 1: 0},  // file3 - no coupling.
		},
	}

	result := m.Compute(input)

	require.Len(t, result, 1)
	assert.Equal(t, testFile1, result[0].File1)
	assert.Equal(t, testFile2, result[0].File2)
}

func TestFileCouplingMetric_OutOfBoundsIndex(t *testing.T) {
	t.Parallel()

	m := NewFileCouplingMetric()
	input := &ReportData{
		Files: []string{testFile1}, // Only 1 file.
		FilesMatrix: []map[int]int64{
			{0: 10, 1: 5, 5: 3}, // References indices beyond Files array.
		},
	}

	result := m.Compute(input)

	// Should not crash and should skip invalid indices.
	assert.Empty(t, result)
}

// --- DeveloperCouplingMetric Tests ---.

func TestNewDeveloperCouplingMetric_Metadata(t *testing.T) {
	t.Parallel()

	m := NewDeveloperCouplingMetric()

	assert.Equal(t, "developer_coupling", m.Name())
	assert.Equal(t, "Developer Coupling", m.DisplayName())
	assert.Contains(t, m.Description(), "pairs of developers")
	assert.Equal(t, "list", m.Type())
}

func TestDeveloperCouplingMetric_Empty(t *testing.T) {
	t.Parallel()

	m := NewDeveloperCouplingMetric()
	input := &ReportData{}

	result := m.Compute(input)

	assert.Empty(t, result)
}

func TestDeveloperCouplingMetric_SinglePair(t *testing.T) {
	t.Parallel()

	m := NewDeveloperCouplingMetric()
	input := &ReportData{
		ReversedPeopleDict: []string{testDev1, testDev2},
		PeopleMatrix: []map[int]int64{
			{0: 20, 1: 10}, // dev1 self=20, shared with dev2=10.
			{0: 10, 1: 15}, // dev2 self=15.
		},
	}

	result := m.Compute(input)

	require.Len(t, result, 1)
	assert.Equal(t, testDev1, result[0].Developer1)
	assert.Equal(t, testDev2, result[0].Developer2)
	assert.Equal(t, int64(10), result[0].SharedFiles)
	// Strength = shared / avg(self_dev1, self_dev2) = 10 / avg(20, 15) = 10/17.5 ≈ 0.571.
	assert.InDelta(t, 10.0/17.5, result[0].Strength, floatDelta)
}

func TestDeveloperCouplingMetric_MultiplePairs_SortedBySharedFiles(t *testing.T) {
	t.Parallel()

	m := NewDeveloperCouplingMetric()
	input := &ReportData{
		ReversedPeopleDict: []string{testDev1, testDev2, testDev3},
		PeopleMatrix: []map[int]int64{
			{0: 20, 1: 5, 2: 15}, // dev1.
			{0: 5, 1: 10, 2: 3},  // dev2.
			{0: 15, 1: 3, 2: 12}, // dev3.
		},
	}

	result := m.Compute(input)

	require.Len(t, result, 3)
	// Should be sorted by SharedFiles descending.
	assert.Equal(t, int64(15), result[0].SharedFiles) // dev1-dev3.
	assert.Equal(t, int64(5), result[1].SharedFiles)  // dev1-dev2.
	assert.Equal(t, int64(3), result[2].SharedFiles)  // dev2-dev3.
}

func TestDeveloperCouplingMetric_MissingDictEntry(t *testing.T) {
	t.Parallel()

	m := NewDeveloperCouplingMetric()
	input := &ReportData{
		ReversedPeopleDict: []string{testDev1}, // Only 1 dev in dict.
		PeopleMatrix: []map[int]int64{
			{0: 20, 1: 10}, // References dev index 1 which is beyond dict.
			{0: 10, 1: 15},
		},
	}

	result := m.Compute(input)

	require.Len(t, result, 1)
	assert.Equal(t, testDev1, result[0].Developer1)
	assert.Empty(t, result[0].Developer2) // Missing from dict.
}

func TestDeveloperCouplingMetric_SkipsZeroSharedChanges(t *testing.T) {
	t.Parallel()

	m := NewDeveloperCouplingMetric()
	input := &ReportData{
		ReversedPeopleDict: []string{testDev1, testDev2},
		PeopleMatrix: []map[int]int64{
			{0: 20}, // No shared changes with dev2.
			{1: 15},
		},
	}

	result := m.Compute(input)

	assert.Empty(t, result)
}

// --- FileOwnershipMetric Tests ---.

func TestNewFileOwnershipMetric_Metadata(t *testing.T) {
	t.Parallel()

	m := NewFileOwnershipMetric()

	assert.Equal(t, "file_ownership", m.Name())
	assert.Equal(t, "File Ownership", m.DisplayName())
	assert.Contains(t, m.Description(), "contributor information")
	assert.Equal(t, "list", m.Type())
}

func TestFileOwnershipMetric_Empty(t *testing.T) {
	t.Parallel()

	m := NewFileOwnershipMetric()
	input := &ReportData{}

	result := m.Compute(input)

	assert.Empty(t, result)
}

func TestFileOwnershipMetric_SingleFile(t *testing.T) {
	t.Parallel()

	m := NewFileOwnershipMetric()
	input := &ReportData{
		Files:       []string{testFile1},
		FilesLines:  []int{100},
		PeopleFiles: [][]int{{0}}, // dev0 touched file0.
	}

	result := m.Compute(input)

	require.Len(t, result, 1)
	assert.Equal(t, testFile1, result[0].File)
	assert.Equal(t, 100, result[0].Lines)
	assert.Equal(t, 1, result[0].Contributors)
}

func TestFileOwnershipMetric_MultipleContributors(t *testing.T) {
	t.Parallel()

	m := NewFileOwnershipMetric()
	input := &ReportData{
		Files:      []string{testFile1, testFile2},
		FilesLines: []int{100, 200},
		PeopleFiles: [][]int{
			{0, 1}, // dev0 touched file0, file1.
			{0},    // dev1 touched file0.
			{1},    // dev2 touched file1.
		},
	}

	result := m.Compute(input)

	require.Len(t, result, 2)
	// file0 - touched by dev0 and dev1.
	assert.Equal(t, testFile1, result[0].File)
	assert.Equal(t, 100, result[0].Lines)
	assert.Equal(t, 2, result[0].Contributors)
	// file1 - touched by dev0 and dev2.
	assert.Equal(t, testFile2, result[1].File)
	assert.Equal(t, 200, result[1].Lines)
	assert.Equal(t, 2, result[1].Contributors)
}

func TestFileOwnershipMetric_MissingFilesLines(t *testing.T) {
	t.Parallel()

	m := NewFileOwnershipMetric()
	input := &ReportData{
		Files:      []string{testFile1, testFile2},
		FilesLines: []int{100}, // Only 1 entry for 2 files.
	}

	result := m.Compute(input)

	require.Len(t, result, 2)
	assert.Equal(t, 100, result[0].Lines)
	assert.Equal(t, 0, result[1].Lines) // Missing, defaults to 0.
}

func TestFileOwnershipMetric_OutOfBoundsFileIndex(t *testing.T) {
	t.Parallel()

	m := NewFileOwnershipMetric()
	input := &ReportData{
		Files:       []string{testFile1},
		FilesLines:  []int{100},
		PeopleFiles: [][]int{{0, 5}}, // Index 5 is out of bounds.
	}

	result := m.Compute(input)

	require.Len(t, result, 1)
	assert.Equal(t, 1, result[0].Contributors) // Only valid index counted.
}

// --- CouplesAggregateMetric Tests ---.

func TestNewAggregateMetric_Metadata(t *testing.T) {
	t.Parallel()

	m := NewAggregateMetric()

	assert.Equal(t, "couples_aggregate", m.Name())
	assert.Equal(t, "Couples Summary", m.DisplayName())
	assert.Contains(t, m.Description(), "Aggregate statistics")
	assert.Equal(t, "aggregate", m.Type())
}

func TestCouplesAggregateMetric_Empty(t *testing.T) {
	t.Parallel()

	m := NewAggregateMetric()
	input := &ReportData{}

	result := m.Compute(input)

	assert.Equal(t, 0, result.TotalFiles)
	assert.Equal(t, 0, result.TotalDevelopers)
	assert.Equal(t, int64(0), result.TotalCoChanges)
	assert.InDelta(t, 0.0, result.AvgCouplingStrength, floatDelta)
	assert.Equal(t, 0, result.HighlyCoupledPairs)
}

func TestCouplesAggregateMetric_WithData(t *testing.T) {
	t.Parallel()

	m := NewAggregateMetric()
	input := &ReportData{
		Files:              []string{testFile1, testFile2, testFile3},
		ReversedPeopleDict: []string{testDev1, testDev2},
		FilesMatrix: []map[int]int64{
			{0: 10, 1: 15, 2: 5}, // file1: coupled with file2=15, file3=5
			{0: 15, 1: 8, 2: 3},  // file2: coupled with file3=3
			{0: 5, 1: 3, 2: 6},   // file3.
		},
	}

	result := m.Compute(input)

	assert.Equal(t, 3, result.TotalFiles)
	assert.Equal(t, 2, result.TotalDevelopers)
	// Total co-changes = 15 + 5 + 3 = 23 (upper triangle only).
	assert.Equal(t, int64(23), result.TotalCoChanges)
	// AvgCouplingStrength = average of per-pair strengths using code-maat formula:
	//   file1-file2: min(15/avg(10,8), 1.0) = min(15/9, 1.0) = 1.0
	//   file1-file3: 5/avg(10,6) = 5/8 = 0.625
	//   file2-file3: 3/avg(8,6) = 3/7 ≈ 0.4286
	//   avg = (1.0 + 0.625 + 0.4286) / 3 ≈ 0.6845
	assert.InDelta(t, (1.0+5.0/8.0+3.0/7.0)/3.0, result.AvgCouplingStrength, floatDelta)
	// Highly coupled pairs (>= 10): 15 only.
	assert.Equal(t, 1, result.HighlyCoupledPairs)
}

func TestCouplesAggregateMetric_HighlyCoupledThreshold(t *testing.T) {
	t.Parallel()

	m := NewAggregateMetric()
	input := &ReportData{
		Files: []string{testFile1, testFile2},
		FilesMatrix: []map[int]int64{
			{0: 10, 1: 10}, // Exactly at threshold.
			{0: 10, 1: 5},
		},
	}

	result := m.Compute(input)

	// 10 is exactly at threshold CouplingThresholdHigh.
	assert.Equal(t, 1, result.HighlyCoupledPairs)
}

// --- ComputeAllMetrics Tests ---.

func TestComputeAllMetrics_Empty(t *testing.T) {
	t.Parallel()

	report := analyze.Report{}

	result, err := ComputeAllMetrics(report)

	require.NoError(t, err)
	assert.Empty(t, result.FileCoupling)
	assert.Empty(t, result.DeveloperCoupling)
	assert.Empty(t, result.FileOwnership)
	assert.Equal(t, 0, result.Aggregate.TotalFiles)
}

func TestComputeAllMetrics_Full(t *testing.T) {
	t.Parallel()

	report := analyze.Report{
		"Files":              []string{testFile1, testFile2},
		"FilesLines":         []int{100, 200},
		"ReversedPeopleDict": []string{testDev1, testDev2},
		"FilesMatrix": []map[int]int64{
			{0: 10, 1: 5},
			{0: 5, 1: 8},
		},
		"PeopleMatrix": []map[int]int64{
			{0: 20, 1: 10},
			{0: 10, 1: 15},
		},
		"PeopleFiles": [][]int{
			{0, 1},
			{0},
		},
	}

	result, err := ComputeAllMetrics(report)

	require.NoError(t, err)

	// FileCoupling.
	require.Len(t, result.FileCoupling, 1)
	assert.Equal(t, testFile1, result.FileCoupling[0].File1)
	assert.Equal(t, testFile2, result.FileCoupling[0].File2)

	// DeveloperCoupling.
	require.Len(t, result.DeveloperCoupling, 1)
	assert.Equal(t, testDev1, result.DeveloperCoupling[0].Developer1)
	assert.Equal(t, testDev2, result.DeveloperCoupling[0].Developer2)

	// FileOwnership.
	require.Len(t, result.FileOwnership, 2)
	assert.Equal(t, testFile1, result.FileOwnership[0].File)
	assert.Equal(t, 2, result.FileOwnership[0].Contributors)

	// Aggregate.
	assert.Equal(t, 2, result.Aggregate.TotalFiles)
	assert.Equal(t, 2, result.Aggregate.TotalDevelopers)
}

// --- MetricsOutput Interface Tests ---.

func TestComputedMetrics_AnalyzerName(t *testing.T) {
	t.Parallel()

	m := &ComputedMetrics{}

	name := m.AnalyzerName()

	assert.Equal(t, "couples", name)
}

func TestComputedMetrics_ToJSON(t *testing.T) {
	t.Parallel()

	m := &ComputedMetrics{
		FileCoupling: []FileCouplingData{{File1: "a.go", File2: "b.go"}},
	}

	result := m.ToJSON()

	assert.Equal(t, m, result)
}

func TestComputedMetrics_ToYAML(t *testing.T) {
	t.Parallel()

	m := &ComputedMetrics{
		FileCoupling: []FileCouplingData{{File1: "a.go", File2: "b.go"}},
	}

	result := m.ToYAML()

	assert.Equal(t, m, result)
}

// --- BucketOwnership Tests ---.

func TestBucketOwnership_Empty(t *testing.T) {
	t.Parallel()

	buckets := BucketOwnership(nil)
	require.Len(t, buckets, 4)

	for _, b := range buckets {
		assert.Zero(t, b.Count)
	}
}

func TestBucketOwnership_AllCategories(t *testing.T) {
	t.Parallel()

	ownership := []FileOwnershipData{
		{File: "a.go", Contributors: 0},  // Single.
		{File: "b.go", Contributors: 1},  // Single.
		{File: "c.go", Contributors: 2},  // Few (2-3).
		{File: "d.go", Contributors: 3},  // Few (2-3).
		{File: "e.go", Contributors: 4},  // Moderate (4-5).
		{File: "f.go", Contributors: 5},  // Moderate (4-5).
		{File: "g.go", Contributors: 6},  // Many (6+).
		{File: "h.go", Contributors: 10}, // Many (6+).
	}

	buckets := BucketOwnership(ownership)
	require.Len(t, buckets, 4)

	assert.Equal(t, "Single owner", buckets[0].Label)
	assert.Equal(t, 2, buckets[0].Count)

	assert.Equal(t, "2-3 owners", buckets[1].Label)
	assert.Equal(t, 2, buckets[1].Count)

	assert.Equal(t, "4-5 owners", buckets[2].Label)
	assert.Equal(t, 2, buckets[2].Count)

	assert.Equal(t, "6+ owners", buckets[3].Label)
	assert.Equal(t, 2, buckets[3].Count)
}

// --- SortOwnershipByRisk Tests ---.

func TestSortOwnershipByRisk_SortsAscending(t *testing.T) {
	t.Parallel()

	ownership := []FileOwnershipData{
		{File: "a.go", Contributors: 5},
		{File: "b.go", Contributors: 1},
		{File: "c.go", Contributors: 3},
	}

	sorted := SortOwnershipByRisk(ownership)
	require.Len(t, sorted, 3)
	assert.Equal(t, "b.go", sorted[0].File) // 1 = highest risk.
	assert.Equal(t, "c.go", sorted[1].File) // 3
	assert.Equal(t, "a.go", sorted[2].File) // 5 = lowest risk.
}

func TestSortOwnershipByRisk_DoesNotMutateOriginal(t *testing.T) {
	t.Parallel()

	ownership := []FileOwnershipData{
		{File: "a.go", Contributors: 5},
		{File: "b.go", Contributors: 1},
	}

	sorted := SortOwnershipByRisk(ownership)

	// Original should remain unchanged.
	assert.Equal(t, "a.go", ownership[0].File)
	assert.Equal(t, "b.go", sorted[0].File)
}

// --- FilterTopDevs Tests ---.

func TestFilterTopDevs_UnderLimit(t *testing.T) {
	t.Parallel()

	matrix := []map[int]int64{
		{0: 10, 1: 3},
		{0: 3, 1: 20},
	}
	names := []string{"alice", "bob"}

	fm, fn := FilterTopDevs(matrix, names, 5)
	assert.Equal(t, matrix, fm)
	assert.Equal(t, names, fn)
}

func TestFilterTopDevs_OverLimit(t *testing.T) {
	t.Parallel()

	// 3 devs, limit to 2: charlie (diag=30) and bob (diag=20) should remain.
	matrix := []map[int]int64{
		{0: 10, 1: 5, 2: 2},
		{0: 5, 1: 20, 2: 8},
		{0: 2, 1: 8, 2: 30},
	}
	names := []string{"alice", "bob", "charlie"}

	fm, fn := FilterTopDevs(matrix, names, 2)
	require.Len(t, fn, 2)
	require.Len(t, fm, 2)

	// Top 2 by diagonal: charlie (30), bob (20).
	assert.Equal(t, "charlie", fn[0])
	assert.Equal(t, "bob", fn[1])

	// Verify coupling between charlie and bob is preserved.
	assert.Equal(t, int64(8), fm[0][1])
	assert.Equal(t, int64(8), fm[1][0])
}
