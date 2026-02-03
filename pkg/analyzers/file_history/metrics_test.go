package filehistory

import (
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
	pkgplumbing "github.com/Sumatoshi-tech/codefang/pkg/plumbing"
)

// Test constants to avoid magic strings/numbers.
const (
	testFile1 = "file1.go"
	testFile2 = "file2.go"
	testFile3 = "file3.go"

	testDevID1 = 1
	testDevID2 = 2
	testDevID3 = 3

	floatDelta = 0.01
)

// Helper function to create test hash.
func testHash(s string) gitlib.Hash {
	var h gitlib.Hash
	copy(h[:], s)
	return h
}

// Helper function to create test hashes.
func testHashes(count int) []gitlib.Hash {
	hashes := make([]gitlib.Hash, count)
	for i := 0; i < count; i++ {
		hashes[i] = testHash(string(rune('a' + i)))
	}
	return hashes
}

// --- ParseReportData Tests ---

func TestParseReportData_Empty(t *testing.T) {
	report := analyze.Report{}

	result, err := ParseReportData(report)

	require.NoError(t, err)
	assert.Empty(t, result.Files)
}

func TestParseReportData_WithFiles(t *testing.T) {
	files := map[string]FileHistory{
		testFile1: {
			People: map[int]pkgplumbing.LineStats{
				testDevID1: {Added: 100, Removed: 20, Changed: 30},
			},
			Hashes: testHashes(5),
		},
	}

	report := analyze.Report{
		"Files": files,
	}

	result, err := ParseReportData(report)

	require.NoError(t, err)
	require.Len(t, result.Files, 1)

	fh, ok := result.Files[testFile1]
	require.True(t, ok)
	assert.Len(t, fh.Hashes, 5)
	assert.Len(t, fh.People, 1)
	assert.Equal(t, 100, fh.People[testDevID1].Added)
}

// --- FileChurnMetric Tests ---

func TestNewFileChurnMetric_Metadata(t *testing.T) {
	m := NewFileChurnMetric()

	assert.Equal(t, "file_churn", m.Name())
	assert.Equal(t, "File Churn Statistics", m.DisplayName())
	assert.Contains(t, m.Description(), "Per-file change frequency")
	assert.Equal(t, "list", m.Type())
}

func TestFileChurnMetric_Empty(t *testing.T) {
	m := NewFileChurnMetric()
	input := &ReportData{Files: make(map[string]FileHistory)}

	result := m.Compute(input)

	assert.Empty(t, result)
}

func TestFileChurnMetric_SingleFile(t *testing.T) {
	m := NewFileChurnMetric()
	input := &ReportData{
		Files: map[string]FileHistory{
			testFile1: {
				People: map[int]pkgplumbing.LineStats{
					testDevID1: {Added: 100, Removed: 20, Changed: 30},
					testDevID2: {Added: 50, Removed: 10, Changed: 15},
				},
				Hashes: testHashes(10),
			},
		},
	}

	result := m.Compute(input)

	require.Len(t, result, 1)
	assert.Equal(t, testFile1, result[0].Path)
	assert.Equal(t, 10, result[0].CommitCount)
	assert.Equal(t, 2, result[0].ContributorCount)
	assert.Equal(t, 150, result[0].TotalAdded)  // 100 + 50
	assert.Equal(t, 30, result[0].TotalRemoved) // 20 + 10
	assert.Equal(t, 45, result[0].TotalChanged) // 30 + 15
	// ChurnScore = 10 + (150+30+45)/100 = 10 + 2.25 = 12.25
	assert.InDelta(t, 12.25, result[0].ChurnScore, floatDelta)
}

func TestFileChurnMetric_SortedByChurnScore(t *testing.T) {
	m := NewFileChurnMetric()
	input := &ReportData{
		Files: map[string]FileHistory{
			testFile1: {
				People: map[int]pkgplumbing.LineStats{testDevID1: {Added: 10}},
				Hashes: testHashes(5), // Low churn
			},
			testFile2: {
				People: map[int]pkgplumbing.LineStats{testDevID1: {Added: 1000}},
				Hashes: testHashes(20), // High churn
			},
			testFile3: {
				People: map[int]pkgplumbing.LineStats{testDevID1: {Added: 100}},
				Hashes: testHashes(10), // Medium churn
			},
		},
	}

	result := m.Compute(input)

	require.Len(t, result, 3)
	// Sorted by churn score descending
	assert.Equal(t, testFile2, result[0].Path) // 20 + 10 = 30
	assert.Equal(t, testFile3, result[1].Path) // 10 + 1 = 11
	assert.Equal(t, testFile1, result[2].Path) // 5 + 0.1 = 5.1
}

// --- FileContributorMetric Tests ---

func TestNewFileContributorMetric_Metadata(t *testing.T) {
	m := NewFileContributorMetric()

	assert.Equal(t, "file_contributors", m.Name())
	assert.Equal(t, "File Contributor Breakdown", m.DisplayName())
	assert.Contains(t, m.Description(), "breakdown of which developers")
	assert.Equal(t, "list", m.Type())
}

func TestFileContributorMetric_Empty(t *testing.T) {
	m := NewFileContributorMetric()
	input := &ReportData{Files: make(map[string]FileHistory)}

	result := m.Compute(input)

	assert.Empty(t, result)
}

func TestFileContributorMetric_SingleFile(t *testing.T) {
	m := NewFileContributorMetric()
	input := &ReportData{
		Files: map[string]FileHistory{
			testFile1: {
				People: map[int]pkgplumbing.LineStats{
					testDevID1: {Added: 50, Changed: 20},  // 70 total
					testDevID2: {Added: 100, Changed: 30}, // 130 total - top
				},
				Hashes: testHashes(5),
			},
		},
	}

	result := m.Compute(input)

	require.Len(t, result, 1)
	assert.Equal(t, testFile1, result[0].Path)
	assert.Len(t, result[0].Contributors, 2)
	assert.Equal(t, testDevID2, result[0].TopContributorID)
	assert.Equal(t, 130, result[0].TopContributorLines)
}

func TestFileContributorMetric_NoContributors(t *testing.T) {
	m := NewFileContributorMetric()
	input := &ReportData{
		Files: map[string]FileHistory{
			testFile1: {
				People: make(map[int]pkgplumbing.LineStats),
				Hashes: testHashes(5),
			},
		},
	}

	result := m.Compute(input)

	require.Len(t, result, 1)
	assert.Equal(t, 0, result[0].TopContributorID)
	assert.Equal(t, 0, result[0].TopContributorLines)
}

// --- HotspotMetric Tests ---

func TestNewHotspotMetric_Metadata(t *testing.T) {
	m := NewHotspotMetric()

	assert.Equal(t, "hotspots", m.Name())
	assert.Equal(t, "Code Hotspots", m.DisplayName())
	assert.Contains(t, m.Description(), "high change frequency")
	assert.Equal(t, "risk", m.Type())
}

func TestHotspotMetric_Empty(t *testing.T) {
	m := NewHotspotMetric()
	input := &ReportData{Files: make(map[string]FileHistory)}

	result := m.Compute(input)

	assert.Empty(t, result)
}

func TestHotspotMetric_BelowThreshold(t *testing.T) {
	m := NewHotspotMetric()
	input := &ReportData{
		Files: map[string]FileHistory{
			testFile1: {
				People: map[int]pkgplumbing.LineStats{testDevID1: {}},
				Hashes: testHashes(10), // Below HotspotThresholdMedium (15)
			},
		},
	}

	result := m.Compute(input)

	assert.Empty(t, result)
}

func TestHotspotMetric_RiskLevels(t *testing.T) {
	tests := []struct {
		name        string
		commitCount int
		expected    string
	}{
		{"critical", 55, RiskCritical},
		{"critical_boundary", 50, RiskCritical},
		{"high", 35, RiskHigh},
		{"high_boundary", 30, RiskHigh},
		{"medium", 20, RiskMedium},
		{"medium_boundary", 15, RiskMedium},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			m := NewHotspotMetric()
			input := &ReportData{
				Files: map[string]FileHistory{
					testFile1: {
						People: map[int]pkgplumbing.LineStats{testDevID1: {}},
						Hashes: testHashes(tt.commitCount),
					},
				},
			}

			result := m.Compute(input)

			require.Len(t, result, 1)
			assert.Equal(t, tt.expected, result[0].RiskLevel)
			assert.Equal(t, tt.commitCount, result[0].CommitCount)
		})
	}
}

func TestHotspotMetric_SortedByRiskThenCommitCount(t *testing.T) {
	m := NewHotspotMetric()
	input := &ReportData{
		Files: map[string]FileHistory{
			testFile1: {
				People: map[int]pkgplumbing.LineStats{testDevID1: {}},
				Hashes: testHashes(20), // MEDIUM
			},
			testFile2: {
				People: map[int]pkgplumbing.LineStats{testDevID1: {}},
				Hashes: testHashes(55), // CRITICAL
			},
			testFile3: {
				People: map[int]pkgplumbing.LineStats{testDevID1: {}},
				Hashes: testHashes(35), // HIGH
			},
		},
	}

	result := m.Compute(input)

	require.Len(t, result, 3)
	// Sorted by risk first (critical > high > medium)
	assert.Equal(t, RiskCritical, result[0].RiskLevel)
	assert.Equal(t, RiskHigh, result[1].RiskLevel)
	assert.Equal(t, RiskMedium, result[2].RiskLevel)
}

// --- riskPriority Tests ---

func TestRiskPriority(t *testing.T) {
	tests := []struct {
		level    string
		expected int
	}{
		{RiskCritical, 0},
		{RiskHigh, 1},
		{RiskMedium, 2},
		{RiskLow, 3},
		{"UNKNOWN", 3},
		{"", 3},
	}

	for _, tt := range tests {
		t.Run(tt.level, func(t *testing.T) {
			result := riskPriority(tt.level)
			assert.Equal(t, tt.expected, result)
		})
	}
}

// --- FileHistoryAggregateMetric Tests ---

func TestNewAggregateMetric_Metadata(t *testing.T) {
	m := NewAggregateMetric()

	assert.Equal(t, "file_history_aggregate", m.Name())
	assert.Equal(t, "File History Summary", m.DisplayName())
	assert.Contains(t, m.Description(), "Aggregate statistics")
	assert.Equal(t, "aggregate", m.Type())
}

func TestFileHistoryAggregateMetric_Empty(t *testing.T) {
	m := NewAggregateMetric()
	input := &ReportData{Files: make(map[string]FileHistory)}

	result := m.Compute(input)

	assert.Equal(t, 0, result.TotalFiles)
	assert.Equal(t, 0, result.TotalCommits)
	assert.Equal(t, 0, result.TotalContributors)
	assert.InDelta(t, 0.0, result.AvgCommitsPerFile, floatDelta)
	assert.InDelta(t, 0.0, result.AvgContributorsPerFile, floatDelta)
	assert.Equal(t, 0, result.HighChurnFiles)
}

func TestFileHistoryAggregateMetric_WithData(t *testing.T) {
	m := NewAggregateMetric()
	input := &ReportData{
		Files: map[string]FileHistory{
			testFile1: {
				People: map[int]pkgplumbing.LineStats{
					testDevID1: {},
					testDevID2: {},
				},
				Hashes: testHashes(20), // High churn
			},
			testFile2: {
				People: map[int]pkgplumbing.LineStats{
					testDevID1: {},
					testDevID3: {},
				},
				Hashes: testHashes(10), // Not high churn
			},
		},
	}

	result := m.Compute(input)

	assert.Equal(t, 2, result.TotalFiles)
	assert.Equal(t, 30, result.TotalCommits)                          // 20 + 10
	assert.Equal(t, 3, result.TotalContributors)                      // devID1, devID2, devID3 (unique)
	assert.InDelta(t, 15.0, result.AvgCommitsPerFile, floatDelta)     // 30/2
	assert.InDelta(t, 2.0, result.AvgContributorsPerFile, floatDelta) // 4/2 (2 per file)
	assert.Equal(t, 1, result.HighChurnFiles)                         // Only file1 >= 15 commits
}

// --- ComputeAllMetrics Tests ---

func TestComputeAllMetrics_Empty(t *testing.T) {
	report := analyze.Report{}

	result, err := ComputeAllMetrics(report)

	require.NoError(t, err)
	assert.Empty(t, result.FileChurn)
	assert.Empty(t, result.FileContributors)
	assert.Empty(t, result.Hotspots)
	assert.Equal(t, 0, result.Aggregate.TotalFiles)
}

// --- MetricsOutput Interface Tests ---

func TestComputedMetrics_AnalyzerName(t *testing.T) {
	m := &ComputedMetrics{}

	name := m.AnalyzerName()

	assert.Equal(t, "file_history", name)
}

func TestComputedMetrics_ToJSON(t *testing.T) {
	m := &ComputedMetrics{
		FileChurn: []FileChurnData{{Path: "test.go"}},
	}

	result := m.ToJSON()

	assert.Equal(t, m, result)
}

func TestComputedMetrics_ToYAML(t *testing.T) {
	m := &ComputedMetrics{
		FileChurn: []FileChurnData{{Path: "test.go"}},
	}

	result := m.ToYAML()

	assert.Equal(t, m, result)
}

func TestComputeAllMetrics_Full(t *testing.T) {
	files := map[string]FileHistory{
		testFile1: {
			People: map[int]pkgplumbing.LineStats{
				testDevID1: {Added: 100, Removed: 10, Changed: 20},
				testDevID2: {Added: 50, Removed: 5, Changed: 10},
			},
			Hashes: testHashes(35), // HIGH risk (>= 30)
		},
		testFile2: {
			People: map[int]pkgplumbing.LineStats{
				testDevID1: {Added: 20},
			},
			Hashes: testHashes(5), // Below threshold
		},
	}

	report := analyze.Report{
		"Files": files,
	}

	result, err := ComputeAllMetrics(report)

	require.NoError(t, err)

	// FileChurn
	require.Len(t, result.FileChurn, 2)

	// FileContributors
	require.Len(t, result.FileContributors, 2)

	// Hotspots - only file1 has >= 15 commits
	require.Len(t, result.Hotspots, 1)
	assert.Equal(t, testFile1, result.Hotspots[0].Path)
	assert.Equal(t, RiskHigh, result.Hotspots[0].RiskLevel)

	// Aggregate
	assert.Equal(t, 2, result.Aggregate.TotalFiles)
	assert.Equal(t, 40, result.Aggregate.TotalCommits)     // 35 + 5
	assert.Equal(t, 2, result.Aggregate.TotalContributors) // devID1, devID2
}
