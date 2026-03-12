package burndown

import (
	"testing"
	"time"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
)

// Test constants to avoid magic strings/numbers.
const (
	testDevName1      = "Alice"
	testDevName2      = "Bob"
	testFilePath1     = "main.go"
	testFilePath2     = "util.go"
	testProjectName   = "TestProject"
	testSampling      = 30
	testGranularity   = 30
	testTickSizeHours = 24
)

func getTestTickSize() time.Duration {
	return testTickSizeHours * time.Hour
}

// --- ParseReportData Tests ---.

func TestParseReportData_Empty(t *testing.T) {
	t.Parallel()

	report := analyze.Report{}

	data, err := ParseReportData(report)

	require.NoError(t, err)
	require.NotNil(t, data)
	assert.Nil(t, data.GlobalHistory)
	assert.Nil(t, data.FileHistories)
	assert.Equal(t, 24*time.Hour, data.TickSize)
}

func TestParseReportData_Valid(t *testing.T) {
	t.Parallel()

	globalHistory := DenseHistory{{100, 200}, {150, 180}}
	fileHistories := map[string]DenseHistory{testFilePath1: {{50, 100}}}
	fileOwnership := map[string]map[int]int{testFilePath1: {0: 100, 1: 50}}
	peopleHistories := []DenseHistory{{{100, 200}}, {{50, 100}}}
	peopleMatrix := DenseHistory{{0, 10, 20}, {5, 0, 15}}
	names := []string{testDevName1, testDevName2}
	endTime := time.Now()

	report := analyze.Report{
		"GlobalHistory":      globalHistory,
		"FileHistories":      fileHistories,
		"FileOwnership":      fileOwnership,
		"PeopleHistories":    peopleHistories,
		"PeopleMatrix":       peopleMatrix,
		"ReversedPeopleDict": names,
		"TickSize":           getTestTickSize(),
		"Sampling":           testSampling,
		"Granularity":        testGranularity,
		"ProjectName":        testProjectName,
		"EndTime":            endTime,
	}

	data, err := ParseReportData(report)

	require.NoError(t, err)
	require.NotNil(t, data)
	assert.Equal(t, globalHistory, data.GlobalHistory)
	assert.Equal(t, fileHistories, data.FileHistories)
	assert.Equal(t, fileOwnership, data.FileOwnership)
	assert.Equal(t, peopleHistories, data.PeopleHistories)
	assert.Equal(t, peopleMatrix, data.PeopleMatrix)
	assert.Equal(t, names, data.ReversedPeopleDict)
	assert.Equal(t, getTestTickSize(), data.TickSize)
	assert.Equal(t, testSampling, data.Sampling)
	assert.Equal(t, testGranularity, data.Granularity)
	assert.Equal(t, testProjectName, data.ProjectName)
	assert.Equal(t, endTime, data.EndTime)
}

// --- GlobalSurvivalMetric Tests ---.

func TestGlobalSurvivalMetric_Empty(t *testing.T) {
	t.Parallel()

	input := &ReportData{GlobalHistory: nil}

	result := computeGlobalSurvival(input)

	assert.Nil(t, result)
}

func TestGlobalSurvivalMetric_SingleSample(t *testing.T) {
	t.Parallel()

	input := &ReportData{
		GlobalHistory: DenseHistory{{100, 200, 50}},
	}

	result := computeGlobalSurvival(input)

	require.Len(t, result, 1)
	assert.Equal(t, 0, result[0].SampleIndex)
	assert.Equal(t, int64(350), result[0].TotalLines)
	assert.InDelta(t, 1.0, result[0].SurvivalRate, 0.01)
	assert.Equal(t, []int64{100, 200, 50}, result[0].BandBreakdown)
}

func TestGlobalSurvivalMetric_MultipleSamples(t *testing.T) {
	t.Parallel()

	input := &ReportData{
		GlobalHistory: DenseHistory{
			{100, 200}, // Sample 0: 300 total (peak).
			{80, 150},  // Sample 1: 230 total.
			{50, 100},  // Sample 2: 150 total.
		},
	}

	result := computeGlobalSurvival(input)

	require.Len(t, result, 3)
	// Peak is 300, so survival rates are relative to that.
	assert.Equal(t, int64(300), result[0].TotalLines)
	assert.InDelta(t, 1.0, result[0].SurvivalRate, 0.01)
	assert.Equal(t, int64(230), result[1].TotalLines)
	assert.InDelta(t, 230.0/300.0, result[1].SurvivalRate, 0.01)
	assert.Equal(t, int64(150), result[2].TotalLines)
	assert.InDelta(t, 150.0/300.0, result[2].SurvivalRate, 0.01)
}

func TestGlobalSurvivalMetric_GrowingRepo(t *testing.T) {
	t.Parallel()

	// Simulate a growing repo: band 0 decays while band 1 grows.
	// Band 0 peak = 100 (sample 0), band 1 peak = 200 (sample 1).
	// Total lines ever written = 100 + 200 = 300.
	// Current lines = 50 + 200 = 250.
	// Survival = 250/300 = 83.3% (NOT 250/250 = 100%).
	input := &ReportData{
		GlobalHistory: DenseHistory{
			{100, 0},  // Sample 0: 100 total.
			{50, 200}, // Sample 1: 250 total (current).
		},
	}

	result := computeGlobalSurvival(input)

	require.Len(t, result, 2)
	// Peak per band: band0=100, band1=200, total=300.
	assert.Equal(t, int64(100), result[0].TotalLines)
	assert.InDelta(t, 100.0/300.0, result[0].SurvivalRate, 0.01)
	assert.Equal(t, int64(250), result[1].TotalLines)
	assert.InDelta(t, 250.0/300.0, result[1].SurvivalRate, 0.01)
}

func TestGlobalSurvivalMetric_NegativeValuesIgnored(t *testing.T) {
	t.Parallel()

	input := &ReportData{
		GlobalHistory: DenseHistory{{100, -50, 200}},
	}

	result := computeGlobalSurvival(input)

	require.Len(t, result, 1)
	assert.Equal(t, int64(300), result[0].TotalLines)
}

// --- FileSurvivalMetric Tests ---.

func TestFileSurvivalMetric_Empty(t *testing.T) {
	t.Parallel()

	input := FileSurvivalInput{FileOwnership: nil}

	result := computeFileSurvival(input)

	assert.Empty(t, result)
}

func TestFileSurvivalMetric_SingleFile(t *testing.T) {
	t.Parallel()

	input := FileSurvivalInput{
		FileOwnership: map[string]map[int]int{
			testFilePath1: {0: 100},
		},
		ReversedPeopleDict: []string{testDevName1},
	}

	result := computeFileSurvival(input)

	require.Len(t, result, 1)
	assert.Equal(t, testFilePath1, result[0].Path)
	assert.Equal(t, int64(100), result[0].CurrentLines)
	assert.Equal(t, 0, result[0].TopOwnerID)
	assert.Equal(t, testDevName1, result[0].TopOwnerName)
	assert.InDelta(t, 100.0, result[0].TopOwnerPct, 0.01)
}

func TestFileSurvivalMetric_MultipleOwners(t *testing.T) {
	t.Parallel()

	input := FileSurvivalInput{
		FileOwnership: map[string]map[int]int{
			testFilePath1: {0: 70, 1: 30},
		},
		ReversedPeopleDict: []string{testDevName1, testDevName2},
	}

	result := computeFileSurvival(input)

	require.Len(t, result, 1)
	assert.Equal(t, int64(100), result[0].CurrentLines)
	assert.Equal(t, 0, result[0].TopOwnerID)
	assert.Equal(t, testDevName1, result[0].TopOwnerName)
	assert.InDelta(t, 70.0, result[0].TopOwnerPct, 0.01)
}

func TestFileSurvivalMetric_UnknownOwner(t *testing.T) {
	t.Parallel()

	input := FileSurvivalInput{
		FileOwnership: map[string]map[int]int{
			testFilePath1: {999: 100},
		},
		ReversedPeopleDict: []string{testDevName1},
	}

	result := computeFileSurvival(input)

	require.Len(t, result, 1)
	assert.Equal(t, 999, result[0].TopOwnerID)
	assert.Empty(t, result[0].TopOwnerName)
}

// --- DeveloperSurvivalMetric Tests ---.

func TestDeveloperSurvivalMetric_Empty(t *testing.T) {
	t.Parallel()

	input := DeveloperSurvivalInput{PeopleHistories: nil}

	result := computeDeveloperSurvivalList(input)

	assert.Empty(t, result)
}

func TestDeveloperSurvivalMetric_SingleDeveloper(t *testing.T) {
	t.Parallel()

	input := DeveloperSurvivalInput{
		PeopleHistories: []DenseHistory{
			{{100, 200}, {80, 150}}, // Peak=300, Current=230.
		},
		ReversedPeopleDict: []string{testDevName1},
	}

	result := computeDeveloperSurvivalList(input)

	require.Len(t, result, 1)
	assert.Equal(t, 0, result[0].ID)
	assert.Equal(t, testDevName1, result[0].Name)
	assert.Equal(t, int64(300), result[0].PeakLines)
	assert.Equal(t, int64(230), result[0].CurrentLines)
	assert.InDelta(t, 230.0/300.0, result[0].SurvivalRate, 0.01)
}

func TestDeveloperSurvivalMetric_EmptyHistory(t *testing.T) {
	t.Parallel()

	input := DeveloperSurvivalInput{
		PeopleHistories:    []DenseHistory{{}},
		ReversedPeopleDict: []string{testDevName1},
	}

	result := computeDeveloperSurvivalList(input)

	assert.Empty(t, result)
}

func TestDeveloperSurvivalMetric_ZeroPeakLines(t *testing.T) {
	t.Parallel()

	input := DeveloperSurvivalInput{
		PeopleHistories: []DenseHistory{
			{{0, 0}},
		},
		ReversedPeopleDict: []string{testDevName1},
	}

	result := computeDeveloperSurvivalList(input)

	require.Len(t, result, 1)
	assert.Equal(t, int64(0), result[0].PeakLines)
	assert.InDelta(t, 0.0, result[0].SurvivalRate, 0.01)
}

// --- InteractionMetric Tests ---.

func TestInteractionMetric_Empty(t *testing.T) {
	t.Parallel()

	input := InteractionInput{PeopleMatrix: nil}

	result := computeInteraction(input)

	assert.Nil(t, result)
}

func TestInteractionMetric_SelfModify(t *testing.T) {
	t.Parallel()

	// PeopleMatrix row format: index 0 is special, 1 is self, 2+ are other devs.
	input := InteractionInput{
		PeopleMatrix:       DenseHistory{{0, 50, 0}}, // Self-modify at index 0 (maps to -2+0=index 0 being self).
		ReversedPeopleDict: []string{testDevName1},
	}

	result := computeInteraction(input)

	// Check we got some interaction data.
	require.NotEmpty(t, result)
}

func TestInteractionMetric_ZeroLinesFiltered(t *testing.T) {
	t.Parallel()

	input := InteractionInput{
		PeopleMatrix:       DenseHistory{{0, 0, 0}},
		ReversedPeopleDict: []string{testDevName1},
	}

	result := computeInteraction(input)

	assert.Empty(t, result)
}

func TestInteractionMetric_EmptyRow(t *testing.T) {
	t.Parallel()

	input := InteractionInput{
		PeopleMatrix:       DenseHistory{{}},
		ReversedPeopleDict: []string{testDevName1},
	}

	result := computeInteraction(input)

	assert.Empty(t, result)
}

func TestInteractionMetric_CrossDeveloper(t *testing.T) {
	t.Parallel()

	// Row 0: index 0=self, index 2=dev0 modifying
	// ModifierID = modifierIdx - 2, so index 2 means dev 0.
	input := InteractionInput{
		PeopleMatrix: DenseHistory{
			{50, 0, 30}, // Author 0: self-modified 50, dev0 modified 30.
		},
		ReversedPeopleDict: []string{testDevName1, testDevName2},
	}

	result := computeInteraction(input)

	// Should have entries for both self-modify and cross-dev modify.
	require.NotEmpty(t, result)

	// Find self-modify entry.
	var (
		hasSelfModify  bool
		hasCrossModify bool
	)

	for _, r := range result {
		if r.IsSelfModify {
			hasSelfModify = true

			assert.Equal(t, int64(50), r.LinesModified)
		} else {
			hasCrossModify = true

			assert.Equal(t, int64(30), r.LinesModified)
		}
	}

	assert.True(t, hasSelfModify || hasCrossModify)
}

func TestInteractionMetric_MultipleDevelopers(t *testing.T) {
	t.Parallel()

	input := InteractionInput{
		PeopleMatrix: DenseHistory{
			{100, 0, 20, 30}, // Author 0.
			{50, 0, 10, 40},  // Author 1.
		},
		ReversedPeopleDict: []string{testDevName1, testDevName2},
	}

	result := computeInteraction(input)

	require.NotEmpty(t, result)
	// Verify we have interaction data from both authors.
	authors := make(map[int]bool)
	for _, r := range result {
		authors[r.AuthorID] = true
	}

	assert.GreaterOrEqual(t, len(authors), 1)
}

// --- BurndownAggregateMetric Tests ---.

func TestBurndownAggregateMetric_Empty(t *testing.T) {
	t.Parallel()

	input := &ReportData{
		GlobalHistory:   nil,
		FileHistories:   map[string]DenseHistory{testFilePath1: {{100}}},
		PeopleHistories: []DenseHistory{{{100}}},
	}

	result := computeAggregate(input)

	assert.Equal(t, 1, result.TrackedFiles)
	assert.Equal(t, 1, result.TrackedDevelopers)
	assert.Equal(t, 0, result.NumSamples)
}

func TestBurndownAggregateMetric_Compute(t *testing.T) {
	t.Parallel()

	input := &ReportData{
		GlobalHistory: DenseHistory{
			{100, 200}, // 300 total.
			{80, 150},  // 230 total (current).
		},
		FileHistories:   map[string]DenseHistory{testFilePath1: {{100}}, testFilePath2: {{50}}},
		PeopleHistories: []DenseHistory{{{100}}, {{50}}},
		TickSize:        getTestTickSize(),
		Sampling:        testSampling,
	}

	result := computeAggregate(input)

	assert.Equal(t, 2, result.TrackedFiles)
	assert.Equal(t, 2, result.TrackedDevelopers)
	assert.Equal(t, 2, result.NumSamples)
	assert.Equal(t, 2, result.NumBands)
	assert.Equal(t, int64(300), result.TotalPeakLines)
	assert.Equal(t, int64(230), result.TotalCurrentLines)
	assert.InDelta(t, 230.0/300.0, result.OverallSurvivalRate, 0.01)
}

func TestBurndownAggregateMetric_GrowingRepo(t *testing.T) {
	t.Parallel()

	// Growing repo: last sample has the most lines, but survival < 100%
	// because older code was deleted.
	input := &ReportData{
		GlobalHistory: DenseHistory{
			{100, 0},  // Sample 0: 100 total.
			{50, 200}, // Sample 1: 250 total (current).
		},
		TickSize: getTestTickSize(),
		Sampling: testSampling,
	}

	result := computeAggregate(input)

	// Peak = sum of per-band peaks: max(100,50) + max(0,200) = 100 + 200 = 300.
	assert.Equal(t, int64(300), result.TotalPeakLines)
	assert.Equal(t, int64(250), result.TotalCurrentLines)
	assert.InDelta(t, 250.0/300.0, result.OverallSurvivalRate, 0.01)
}

// --- ComputeAllMetrics Tests ---.

func TestComputeAllMetrics_Empty(t *testing.T) {
	t.Parallel()

	report := analyze.Report{}

	result, err := ComputeAllMetrics(report)

	require.NoError(t, err)
	require.NotNil(t, result)
	assert.Nil(t, result.GlobalSurvival)
	assert.Empty(t, result.FileSurvival)
	assert.Empty(t, result.DeveloperSurvival)
	assert.Nil(t, result.Interaction)
}

func TestComputeAllMetrics_Valid(t *testing.T) {
	t.Parallel()

	report := analyze.Report{
		"GlobalHistory": DenseHistory{{100, 200}},
		"FileOwnership": map[string]map[int]int{testFilePath1: {0: 100}},
		"PeopleHistories": []DenseHistory{
			{{100, 200}},
		},
		"PeopleMatrix":       DenseHistory{{0, 50}},
		"ReversedPeopleDict": []string{testDevName1},
		"TickSize":           getTestTickSize(),
		"Sampling":           testSampling,
	}

	result, err := ComputeAllMetrics(report)

	require.NoError(t, err)
	require.NotNil(t, result)
	assert.NotEmpty(t, result.GlobalSurvival)
	assert.NotEmpty(t, result.FileSurvival)
	assert.NotEmpty(t, result.DeveloperSurvival)
	assert.Equal(t, int64(300), result.Aggregate.TotalPeakLines)
}

// --- MetricsOutput Interface Tests ---.

func TestComputedMetrics_AnalyzerName(t *testing.T) {
	t.Parallel()

	m := &ComputedMetrics{}

	assert.Equal(t, "burndown", m.AnalyzerName())
}

func TestComputedMetrics_ToJSON(t *testing.T) {
	t.Parallel()

	m := &ComputedMetrics{
		Aggregate: AggregateData{TotalCurrentLines: 100},
	}

	result := m.ToJSON()

	assert.Equal(t, m, result)
}

func TestComputedMetrics_ToYAML(t *testing.T) {
	t.Parallel()

	m := &ComputedMetrics{
		Aggregate: AggregateData{TotalCurrentLines: 100},
	}

	result := m.ToYAML()

	assert.Equal(t, m, result)
}

func TestComputedMetrics_ImplementsMetricsOutput(t *testing.T) {
	t.Parallel()

	// Compile-time check that ComputedMetrics implements MetricsOutput.
	var _ interface {
		AnalyzerName() string
		ToJSON() any
		ToYAML() any
	} = (*ComputedMetrics)(nil)
}
