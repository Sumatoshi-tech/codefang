package devs

import (
	"testing"
	"time"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/common/renderer"
	pkgplumbing "github.com/Sumatoshi-tech/codefang/internal/plumbing"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

// Test constants to avoid magic strings/numbers.
const (
	testDevName1     = "Alice"
	testDevName2     = "Bob"
	testDevName3     = "Charlie"
	testLangGo       = "Go"
	testLangPython   = "Python"
	testLangOther    = "Other"
	testCommits      = 10
	testLinesAdded   = 100
	testLinesRemoved = 50
	testTickSize     = 24 * time.Hour
	testHashA        = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
	testHashB        = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
)

// --- ParseTickData Tests ---.

func TestParseTickData_Valid(t *testing.T) {
	t.Parallel()

	commitDevData := map[string]*CommitDevData{
		testHashA: {Commits: testCommits, AuthorID: 0},
	}
	commitsByTick := map[int][]gitlib.Hash{
		0: {gitlib.NewHash(testHashA)},
	}
	names := []string{testDevName1}

	report := analyze.Report{
		"CommitDevData":      commitDevData,
		"CommitsByTick":      commitsByTick,
		"ReversedPeopleDict": names,
		"TickSize":           testTickSize,
	}

	data, err := ParseTickData(report)

	require.NoError(t, err)
	require.NotNil(t, data)
	require.Len(t, data.Ticks, 1)
	require.Contains(t, data.Ticks[0], 0)
	assert.Equal(t, testCommits, data.Ticks[0][0].Commits)
	assert.Equal(t, names, data.Names)
	assert.Equal(t, testTickSize, data.TickSize)
}

func TestParseTickData_EmptyCanonical(t *testing.T) {
	t.Parallel()

	report := analyze.Report{
		"ReversedPeopleDict": []string{testDevName1},
		"TickSize":           testTickSize,
	}

	data, err := ParseTickData(report)

	require.NoError(t, err)
	require.NotNil(t, data)
	assert.Empty(t, data.Ticks)
	assert.Equal(t, []string{testDevName1}, data.Names)
}

func TestParseTickData_MissingPeopleDict(t *testing.T) {
	t.Parallel()

	report := analyze.Report{
		"CommitDevData": map[string]*CommitDevData{},
		"CommitsByTick": map[int][]gitlib.Hash{},
		"TickSize":      testTickSize,
	}

	data, err := ParseTickData(report)

	require.Error(t, err)
	assert.Equal(t, ErrInvalidPeopleDict, err)
	assert.Nil(t, data)
}

func TestParseTickData_MissingTickSize_DefaultsTo24Hours(t *testing.T) {
	t.Parallel()

	names := []string{testDevName1}

	report := analyze.Report{
		"CommitDevData":      map[string]*CommitDevData{},
		"CommitsByTick":      map[int][]gitlib.Hash{},
		"ReversedPeopleDict": names,
	}

	data, err := ParseTickData(report)

	require.NoError(t, err)
	require.NotNil(t, data)
	assert.Equal(t, defaultTickHours*time.Hour, data.TickSize)
}

// --- DevelopersMetric Tests ---.

func TestDevelopersMetric_Metadata(t *testing.T) {
	t.Parallel()

	metric := NewDevelopersMetric()

	assert.Equal(t, "developers", metric.Name())
	assert.Equal(t, "Developer Statistics", metric.DisplayName())
	assert.Equal(t, "list", metric.Type())
	assert.NotEmpty(t, metric.Description())
}

func TestDevelopersMetric_Empty(t *testing.T) {
	t.Parallel()

	metric := NewDevelopersMetric()
	input := &TickData{
		Ticks: map[int]map[int]*DevTick{},
		Names: []string{},
	}

	result := metric.Compute(input)

	assert.Empty(t, result)
}

func TestDevelopersMetric_SingleDeveloper(t *testing.T) {
	t.Parallel()

	metric := NewDevelopersMetric()
	input := &TickData{
		Ticks: map[int]map[int]*DevTick{
			0: {
				0: {
					LineStats: pkgplumbing.LineStats{Added: testLinesAdded, Removed: testLinesRemoved},
					Commits:   testCommits,
					Languages: map[string]pkgplumbing.LineStats{testLangGo: {Added: testLinesAdded}},
				},
			},
		},
		Names: []string{testDevName1},
	}

	result := metric.Compute(input)

	require.Len(t, result, 1)
	assert.Equal(t, 0, result[0].ID)
	assert.Equal(t, testDevName1, result[0].Name)
	assert.Equal(t, testCommits, result[0].Commits)
	assert.Equal(t, testLinesAdded, result[0].Added)
	assert.Equal(t, testLinesRemoved, result[0].Removed)
	assert.Equal(t, testLinesAdded-testLinesRemoved, result[0].NetLines)
	assert.Equal(t, 0, result[0].FirstTick)
	assert.Equal(t, 0, result[0].LastTick)
	assert.Equal(t, 1, result[0].ActiveTicks)
}

func TestDevelopersMetric_MultipleDevelopers_SortedByCommits(t *testing.T) {
	t.Parallel()

	metric := NewDevelopersMetric()
	input := &TickData{
		Ticks: map[int]map[int]*DevTick{
			0: {
				0: {Commits: 5},  // Alice - fewer commits.
				1: {Commits: 15}, // Bob - more commits.
			},
		},
		Names: []string{testDevName1, testDevName2},
	}

	result := metric.Compute(input)

	require.Len(t, result, 2)
	// Sorted by commits descending.
	assert.Equal(t, testDevName2, result[0].Name)
	assert.Equal(t, 15, result[0].Commits)
	assert.Equal(t, testDevName1, result[1].Name)
	assert.Equal(t, 5, result[1].Commits)
}

func TestDevelopersMetric_MultipleTicks(t *testing.T) {
	t.Parallel()

	metric := NewDevelopersMetric()
	input := &TickData{
		Ticks: map[int]map[int]*DevTick{
			0:  {0: {Commits: 5, LineStats: pkgplumbing.LineStats{Added: 50}}},
			5:  {0: {Commits: 3, LineStats: pkgplumbing.LineStats{Added: 30}}},
			10: {0: {Commits: 2, LineStats: pkgplumbing.LineStats{Added: 20}}},
		},
		Names: []string{testDevName1},
	}

	result := metric.Compute(input)

	require.Len(t, result, 1)
	assert.Equal(t, 10, result[0].Commits) // 5 + 3 + 2
	assert.Equal(t, 100, result[0].Added)  // 50 + 30 + 20
	assert.Equal(t, 0, result[0].FirstTick)
	assert.Equal(t, 10, result[0].LastTick)
	assert.Equal(t, 3, result[0].ActiveTicks)
}

func TestDevelopersMetric_LanguageAggregation(t *testing.T) {
	t.Parallel()

	metric := NewDevelopersMetric()
	input := &TickData{
		Ticks: map[int]map[int]*DevTick{
			0: {
				0: {
					Commits: testCommits,
					Languages: map[string]pkgplumbing.LineStats{
						testLangGo:     {Added: 50, Removed: 10, Changed: 5},
						testLangPython: {Added: 30, Removed: 5, Changed: 2},
					},
				},
			},
			1: {
				0: {
					Commits: testCommits,
					Languages: map[string]pkgplumbing.LineStats{
						testLangGo: {Added: 20, Removed: 5, Changed: 3},
					},
				},
			},
		},
		Names: []string{testDevName1},
	}

	result := metric.Compute(input)

	require.Len(t, result, 1)
	require.NotNil(t, result[0].Languages)
	assert.Equal(t, 70, result[0].Languages[testLangGo].Added)   // 50 + 20
	assert.Equal(t, 15, result[0].Languages[testLangGo].Removed) // 10 + 5
	assert.Equal(t, 8, result[0].Languages[testLangGo].Changed)  // 5 + 3
	assert.Equal(t, 30, result[0].Languages[testLangPython].Added)
	assert.Equal(t, 5, result[0].Languages[testLangPython].Removed)
	assert.Equal(t, 2, result[0].Languages[testLangPython].Changed)
}

func TestDevelopersMetric_ChangedField(t *testing.T) {
	t.Parallel()

	metric := NewDevelopersMetric()
	input := &TickData{
		Ticks: map[int]map[int]*DevTick{
			0: {
				0: {
					LineStats: pkgplumbing.LineStats{Added: 100, Removed: 50, Changed: 25},
					Commits:   testCommits,
				},
			},
		},
		Names: []string{testDevName1},
	}

	result := metric.Compute(input)

	require.Len(t, result, 1)
	assert.Equal(t, 25, result[0].Changed)
}

// --- LanguagesMetric Tests ---.

func TestLanguagesMetric_Metadata(t *testing.T) {
	t.Parallel()

	metric := NewLanguagesMetric()

	assert.Equal(t, "languages", metric.Name())
	assert.Equal(t, "Language Statistics", metric.DisplayName())
	assert.Equal(t, "list", metric.Type())
	assert.NotEmpty(t, metric.Description())
}

func TestLanguagesMetric_Empty(t *testing.T) {
	t.Parallel()

	metric := NewLanguagesMetric()

	result := metric.Compute([]DeveloperData{})

	assert.Empty(t, result)
}

func TestLanguagesMetric_SingleLanguage(t *testing.T) {
	t.Parallel()

	developers := []DeveloperData{
		{
			ID:        0,
			Languages: map[string]pkgplumbing.LineStats{testLangGo: {Added: testLinesAdded}},
		},
	}
	metric := NewLanguagesMetric()

	result := metric.Compute(developers)

	require.Len(t, result, 1)
	assert.Equal(t, testLangGo, result[0].Name)
	assert.Equal(t, testLinesAdded, result[0].TotalLines)
	assert.Equal(t, testLinesAdded, result[0].TotalContribution) // Contribution = added+removed = 100+0.
	assert.Equal(t, testLinesAdded, result[0].Contributors[0])   // Per-dev contribution.
}

func TestLanguagesMetric_MultipleLanguages_SortedByTotalLines(t *testing.T) {
	t.Parallel()

	developers := []DeveloperData{
		{
			ID: 0,
			Languages: map[string]pkgplumbing.LineStats{
				testLangGo:     {Added: 50},
				testLangPython: {Added: 150},
			},
		},
	}
	metric := NewLanguagesMetric()

	result := metric.Compute(developers)

	require.Len(t, result, 2)
	// Sorted by TotalLines descending.
	assert.Equal(t, testLangPython, result[0].Name)
	assert.Equal(t, 150, result[0].TotalLines)
	assert.Equal(t, testLangGo, result[1].Name)
	assert.Equal(t, 50, result[1].TotalLines)
}

func TestLanguagesMetric_EmptyLanguageName_BecomesOther(t *testing.T) {
	t.Parallel()

	developers := []DeveloperData{
		{
			ID:        0,
			Languages: map[string]pkgplumbing.LineStats{"": {Added: testLinesAdded}},
		},
	}
	metric := NewLanguagesMetric()

	result := metric.Compute(developers)

	require.Len(t, result, 1)
	assert.Equal(t, testLangOther, result[0].Name)
}

func TestLanguagesMetric_MultipleContributors(t *testing.T) {
	t.Parallel()

	developers := []DeveloperData{
		{ID: 0, Languages: map[string]pkgplumbing.LineStats{testLangGo: {Added: 60}}},
		{ID: 1, Languages: map[string]pkgplumbing.LineStats{testLangGo: {Added: 40}}},
	}
	metric := NewLanguagesMetric()

	result := metric.Compute(developers)

	require.Len(t, result, 1)
	assert.Equal(t, testLangGo, result[0].Name)
	assert.Equal(t, 100, result[0].TotalLines)
	assert.Equal(t, 100, result[0].TotalContribution) // Total contribution = (60+0)+(40+0).
	assert.Equal(t, 60, result[0].Contributors[0])    // Per-dev contribution.
	assert.Equal(t, 40, result[0].Contributors[1])
}

func TestLanguagesMetric_ContributionIncludesRemoved(t *testing.T) {
	t.Parallel()

	developers := []DeveloperData{
		{ID: 0, Languages: map[string]pkgplumbing.LineStats{testLangGo: {Added: 60, Removed: 40}}},
		{ID: 1, Languages: map[string]pkgplumbing.LineStats{testLangGo: {Added: 10, Removed: 90}}},
	}
	metric := NewLanguagesMetric()

	result := metric.Compute(developers)

	require.Len(t, result, 1)
	assert.Equal(t, 70, result[0].TotalLines)         // Added only: 60+10.
	assert.Equal(t, 200, result[0].TotalContribution) // (60+40)+(10+90).
	assert.Equal(t, 100, result[0].Contributors[0])   // 60+40.
	assert.Equal(t, 100, result[0].Contributors[1])   // 10+90.
}

// --- BusFactorMetric Tests ---.

func TestBusFactorMetric_Metadata(t *testing.T) {
	t.Parallel()

	metric := NewBusFactorMetric()

	assert.Equal(t, "bus_factor", metric.Name())
	assert.Equal(t, "Bus Factor Risk", metric.DisplayName())
	assert.Equal(t, "risk", metric.Type())
	assert.NotEmpty(t, metric.Description())
}

func TestBusFactorMetric_Empty(t *testing.T) {
	t.Parallel()

	metric := NewBusFactorMetric()
	input := BusFactorInput{Languages: []LanguageData{}, Names: []string{}}

	result := metric.Compute(input)

	assert.Empty(t, result)
}

func TestBusFactorMetric_ZeroContribution_Skipped(t *testing.T) {
	t.Parallel()

	metric := NewBusFactorMetric()
	input := BusFactorInput{
		Languages: []LanguageData{{Name: testLangGo, TotalContribution: 0}},
		Names:     []string{testDevName1},
	}

	result := metric.Compute(input)

	assert.Empty(t, result)
}

func TestBusFactorMetric_SingleContributor_Critical(t *testing.T) {
	t.Parallel()

	metric := NewBusFactorMetric()
	input := BusFactorInput{
		Languages: []LanguageData{
			{Name: testLangGo, TotalLines: 100, TotalContribution: 100, Contributors: map[int]int{0: 100}},
		},
		Names: []string{testDevName1},
	}

	result := metric.Compute(input)

	require.Len(t, result, 1)
	assert.Equal(t, testLangGo, result[0].Language)
	assert.Equal(t, 0, result[0].PrimaryDevID)
	assert.Equal(t, testDevName1, result[0].PrimaryDevName)
	assert.InDelta(t, 100.0, result[0].PrimaryPct, 0.01)
	assert.Equal(t, RiskCritical, result[0].RiskLevel)
	assert.Equal(t, 1, result[0].BusFactor)
	assert.Equal(t, 1, result[0].TotalContributors)
}

func TestBusFactorMetric_RiskLevels(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name       string
		primaryPct int
		wantRisk   string
	}{
		{"CRITICAL - 95%", 95, RiskCritical},
		{"CRITICAL - 90%", 90, RiskCritical},
		{"HIGH - 85%", 85, RiskHigh},
		{"HIGH - 80%", 80, RiskHigh},
		{"MEDIUM - 70%", 70, RiskMedium},
		{"MEDIUM - 60%", 60, RiskMedium},
		{"LOW - 55%", 55, RiskLow},
		{"LOW - 50%", 50, RiskLow},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			metric := NewBusFactorMetric()
			input := BusFactorInput{
				Languages: []LanguageData{
					{
						Name:              testLangGo,
						TotalLines:        100,
						TotalContribution: 100,
						Contributors: map[int]int{
							0: tt.primaryPct,
							1: 100 - tt.primaryPct,
						},
					},
				},
				Names: []string{testDevName1, testDevName2},
			}

			result := metric.Compute(input)

			require.Len(t, result, 1)
			assert.Equal(t, tt.wantRisk, result[0].RiskLevel)
		})
	}
}

func TestBusFactorMetric_SortedByRiskPriority(t *testing.T) {
	t.Parallel()

	metric := NewBusFactorMetric()
	input := BusFactorInput{
		Languages: []LanguageData{
			{Name: testLangGo, TotalLines: 100, TotalContribution: 100, Contributors: map[int]int{0: 50, 1: 50}},    // LOW.
			{Name: testLangPython, TotalLines: 100, TotalContribution: 100, Contributors: map[int]int{0: 95, 1: 5}}, // CRITICAL.
			{Name: "JavaScript", TotalLines: 100, TotalContribution: 100, Contributors: map[int]int{0: 70, 1: 30}},  // MEDIUM.
		},
		Names: []string{testDevName1, testDevName2},
	}

	result := metric.Compute(input)

	require.Len(t, result, 3)
	assert.Equal(t, RiskCritical, result[0].RiskLevel)
	assert.Equal(t, RiskMedium, result[1].RiskLevel)
	assert.Equal(t, RiskLow, result[2].RiskLevel)
}

func TestBusFactorMetric_CHAASSBusFactorNumber(t *testing.T) {
	t.Parallel()

	metric := NewBusFactorMetric()
	input := BusFactorInput{
		Languages: []LanguageData{
			{
				Name:              testLangGo,
				TotalLines:        100,
				TotalContribution: 100,
				Contributors:      map[int]int{0: 30, 1: 25, 2: 20, 3: 15, 4: 10},
			},
		},
		Names: []string{testDevName1, testDevName2, testDevName3, "Dave", "Eve"},
	}

	result := metric.Compute(input)

	require.Len(t, result, 1)
	// Dev0(30) + Dev1(25) = 55 >= 50, so bus factor = 2.
	assert.Equal(t, 2, result[0].BusFactor)
	assert.Equal(t, 5, result[0].TotalContributors)
	assert.Equal(t, RiskLow, result[0].RiskLevel)
}

// --- ActivityMetric Tests ---.

func TestActivityMetric_Metadata(t *testing.T) {
	t.Parallel()

	metric := NewActivityMetric()

	assert.Equal(t, "activity", metric.Name())
	assert.Equal(t, "Commit Activity", metric.DisplayName())
	assert.Equal(t, "time_series", metric.Type())
	assert.NotEmpty(t, metric.Description())
}

func TestActivityMetric_Empty(t *testing.T) {
	t.Parallel()

	metric := NewActivityMetric()
	input := &TickData{Ticks: map[int]map[int]*DevTick{}}

	result := metric.Compute(input)

	assert.Empty(t, result)
}

func TestActivityMetric_SingleTick(t *testing.T) {
	t.Parallel()

	metric := NewActivityMetric()
	input := &TickData{
		Ticks: map[int]map[int]*DevTick{
			0: {0: {Commits: 5}, 1: {Commits: 3}},
		},
	}

	result := metric.Compute(input)

	require.Len(t, result, 1)
	assert.Equal(t, 0, result[0].Tick)
	assert.Equal(t, 8, result[0].TotalCommits)
	assert.Equal(t, 5, result[0].ByDeveloper[0])
	assert.Equal(t, 3, result[0].ByDeveloper[1])
}

func TestActivityMetric_MultipleTicks(t *testing.T) {
	t.Parallel()

	metric := NewActivityMetric()
	input := &TickData{
		Ticks: map[int]map[int]*DevTick{
			0:  {0: {Commits: 5}},
			5:  {0: {Commits: 3}},
			10: {1: {Commits: 2}},
		},
	}

	result := metric.Compute(input)

	require.Len(t, result, 3)
	// Should be sorted by tick.
	assert.Equal(t, 0, result[0].Tick)
	assert.Equal(t, 5, result[1].Tick)
	assert.Equal(t, 10, result[2].Tick)
}

// --- ChurnMetric Tests ---.

func TestChurnMetric_Metadata(t *testing.T) {
	t.Parallel()

	metric := NewChurnMetric()

	assert.Equal(t, "churn", metric.Name())
	assert.Equal(t, "Code Churn", metric.DisplayName())
	assert.Equal(t, "time_series", metric.Type())
	assert.NotEmpty(t, metric.Description())
}

func TestChurnMetric_Empty(t *testing.T) {
	t.Parallel()

	metric := NewChurnMetric()
	input := &TickData{Ticks: map[int]map[int]*DevTick{}}

	result := metric.Compute(input)

	assert.Empty(t, result)
}

func TestChurnMetric_SingleTick(t *testing.T) {
	t.Parallel()

	metric := NewChurnMetric()
	input := &TickData{
		Ticks: map[int]map[int]*DevTick{
			0: {
				0: {LineStats: pkgplumbing.LineStats{Added: 100, Removed: 30}},
				1: {LineStats: pkgplumbing.LineStats{Added: 50, Removed: 20}},
			},
		},
	}

	result := metric.Compute(input)

	require.Len(t, result, 1)
	assert.Equal(t, 0, result[0].Tick)
	assert.Equal(t, 150, result[0].Added)
	assert.Equal(t, 50, result[0].Removed)
	assert.Equal(t, 100, result[0].Net)
}

// --- AggregateMetric Tests ---.

func TestAggregateMetric_Metadata(t *testing.T) {
	t.Parallel()

	metric := NewAggregateMetric()

	assert.Equal(t, "aggregate", metric.Name())
	assert.Equal(t, "Summary Statistics", metric.DisplayName())
	assert.Equal(t, "aggregate", metric.Type())
	assert.NotEmpty(t, metric.Description())
}

func TestAggregateMetric_Empty(t *testing.T) {
	t.Parallel()

	metric := NewAggregateMetric()
	input := AggregateInput{
		Developers: []DeveloperData{},
		Ticks:      map[int]map[int]*DevTick{},
	}

	result := metric.Compute(input)

	assert.Equal(t, 0, result.TotalCommits)
	assert.Equal(t, 0, result.TotalDevelopers)
	assert.Equal(t, 0, result.ActiveDevelopers)
}

func TestAggregateMetric_Compute(t *testing.T) {
	t.Parallel()

	metric := NewAggregateMetric()
	input := AggregateInput{
		Developers: []DeveloperData{
			{Commits: 10, Added: 100, Removed: 30},
			{Commits: 5, Added: 50, Removed: 20},
		},
		Ticks: map[int]map[int]*DevTick{
			0:  {0: {Commits: 5}},
			5:  {0: {Commits: 5}},
			8:  {1: {Commits: 5}}, // Recent.
			10: {0: {Commits: 3}}, // Recent.
		},
		TickSize: testTickSize,
	}

	result := metric.Compute(input)

	assert.Equal(t, 15, result.TotalCommits)
	assert.Equal(t, 150, result.TotalLinesAdded)
	assert.Equal(t, 50, result.TotalLinesRemoved)
	assert.Equal(t, 2, result.TotalDevelopers)
	assert.Equal(t, 10, result.AnalysisPeriodTicks)
	// With TickSize=24h and maxTick=10, 90 days = 90 ticks > 10,
	// so threshold = 0, all devs are active.
	assert.Equal(t, 2, result.ActiveDevelopers)
	// Dev0 has 130 total contribution, Dev1 has 70.
	// 130 >= 100 (50% of 200), so project bus factor = 1.
	assert.Equal(t, 1, result.ProjectBusFactor)
}

func TestAggregateMetric_ActiveDevelopers_TimeBased(t *testing.T) {
	t.Parallel()

	metric := NewAggregateMetric()
	// Ticks span 200 ticks at 24h each = 200 days.
	// Dev 0 only active at tick 0 (old).
	// Dev 1 active at tick 180 (within last 90 days = last 90 ticks).
	input := AggregateInput{
		Developers: []DeveloperData{
			{ID: 0, Commits: 5, Added: 100, Removed: 30},
			{ID: 1, Commits: 3, Added: 50, Removed: 10},
		},
		Ticks: map[int]map[int]*DevTick{
			0:   {0: {Commits: 5}},
			180: {1: {Commits: 3}},
		},
		TickSize: testTickSize,
	}

	result := metric.Compute(input)

	// Threshold = 180 - 90 = 90. Dev 0 at tick 0 < 90 => inactive.
	// Dev 1 at tick 180 >= 90 => active.
	assert.Equal(t, 180, result.AnalysisPeriodTicks)
	assert.Equal(t, 1, result.ActiveDevelopers)
}

func TestAggregateMetric_ActiveDevelopers_RatioFallback(t *testing.T) {
	t.Parallel()

	metric := NewAggregateMetric()
	// No TickSize => falls back to ratio-based (last 30%).
	input := AggregateInput{
		Developers: []DeveloperData{
			{ID: 0, Commits: 5, Added: 100, Removed: 30},
			{ID: 1, Commits: 3, Added: 50, Removed: 10},
		},
		Ticks: map[int]map[int]*DevTick{
			0:  {0: {Commits: 5}},
			5:  {0: {Commits: 5}},
			8:  {1: {Commits: 5}}, // Recent (8 >= 10*0.7=7).
			10: {0: {Commits: 3}}, // Recent.
		},
	}

	result := metric.Compute(input)

	// Threshold = 10 * 0.7 = 7. Both devs active at tick >= 7.
	assert.Equal(t, 2, result.ActiveDevelopers)
}

// --- riskPriority Tests ---.

func TestRiskPriority(t *testing.T) {
	t.Parallel()

	tests := []struct {
		level    string
		priority int
	}{
		{RiskCritical, 0},
		{RiskHigh, 1},
		{RiskMedium, 2},
		{RiskLow, 3},
		{"UNKNOWN", 3},
	}

	for _, tt := range tests {
		t.Run(tt.level, func(t *testing.T) {
			t.Parallel()

			got := riskPriority(tt.level)

			assert.Equal(t, tt.priority, got)
		})
	}
}

// --- ComputeAllMetrics Tests ---.

func TestComputeAllMetrics_InvalidReport(t *testing.T) {
	t.Parallel()

	report := analyze.Report{}

	result, err := ComputeAllMetrics(report)

	require.Error(t, err)
	assert.Nil(t, result)
}

func TestComputeAllMetrics_Valid(t *testing.T) {
	t.Parallel()

	report := analyze.Report{
		"CommitDevData": map[string]*CommitDevData{
			testHashA: {
				Commits:   5,
				Added:     100,
				Removed:   30,
				Changed:   0,
				AuthorID:  0,
				Languages: map[string]pkgplumbing.LineStats{testLangGo: {Added: 100}},
			},
		},
		"CommitsByTick":      map[int][]gitlib.Hash{0: {gitlib.NewHash(testHashA)}},
		"ReversedPeopleDict": []string{testDevName1},
		"TickSize":           testTickSize,
	}

	result, err := ComputeAllMetrics(report)

	require.NoError(t, err)
	require.NotNil(t, result)

	// Verify all metrics computed.
	assert.NotEmpty(t, result.Developers)
	assert.NotEmpty(t, result.Languages)
	assert.NotEmpty(t, result.BusFactor)
	assert.NotEmpty(t, result.Activity)
	assert.NotEmpty(t, result.Churn)
	assert.Equal(t, 5, result.Aggregate.TotalCommits)
}

// --- MetricsOutput Interface Tests ---.

const expectedAnalyzerName = "devs"

func TestComputedMetrics_AnalyzerName(t *testing.T) {
	t.Parallel()

	metrics := &ComputedMetrics{}

	got := metrics.AnalyzerName()

	assert.Equal(t, expectedAnalyzerName, got)
}

func TestComputedMetrics_ToJSON(t *testing.T) {
	t.Parallel()

	metrics := &ComputedMetrics{
		Aggregate: AggregateData{TotalCommits: testCommits},
	}

	result := metrics.ToJSON()

	// Should return the metrics struct itself.
	got, ok := result.(*ComputedMetrics)
	require.True(t, ok, "ToJSON should return *ComputedMetrics")
	assert.Equal(t, testCommits, got.Aggregate.TotalCommits)
}

func TestComputedMetrics_ToYAML(t *testing.T) {
	t.Parallel()

	metrics := &ComputedMetrics{
		Aggregate: AggregateData{TotalCommits: testCommits},
	}

	result := metrics.ToYAML()

	// Should return the metrics struct itself.
	got, ok := result.(*ComputedMetrics)
	require.True(t, ok, "ToYAML should return *ComputedMetrics")
	assert.Equal(t, testCommits, got.Aggregate.TotalCommits)
}

func TestComputedMetrics_ImplementsMetricsOutput(t *testing.T) {
	t.Parallel()

	metrics := &ComputedMetrics{}

	// Compile-time interface compliance check.
	var _ renderer.MetricsOutput = metrics
}

func TestIntVal(t *testing.T) {
	t.Parallel()

	assert.Equal(t, 42, intVal(float64(42)))
	assert.Equal(t, 10, intVal(10))
	assert.Equal(t, 0, intVal("not a number"))
	assert.Equal(t, 0, intVal(nil))
}

func TestBuildCommitDevDataFromMap(t *testing.T) {
	t.Parallel()

	cddMap := map[string]any{
		"hash1": map[string]any{
			"commits":       float64(1),
			"lines_added":   float64(20),
			"lines_removed": float64(5),
			"lines_changed": float64(3),
			"author_id":     float64(0),
			"languages": map[string]any{
				"Go": map[string]any{
					"added":   float64(20),
					"removed": float64(5),
					"changed": float64(3),
				},
			},
		},
	}

	result := buildCommitDevDataFromMap(cddMap)
	require.Len(t, result, 1)
	assert.Equal(t, 1, result["hash1"].Commits)
	assert.Equal(t, 20, result["hash1"].Added)
	assert.Equal(t, 20, result["hash1"].Languages["Go"].Added)
}

func TestBuildCommitsByTickFromMap(t *testing.T) {
	t.Parallel()

	cbtMap := map[string]any{
		"0": []any{testHashA},
		"1": []any{testHashB},
	}

	result := buildCommitsByTickFromMap(cbtMap)
	require.Len(t, result, 2)
	assert.Len(t, result[0], 1)
	assert.Len(t, result[1], 1)
}

func TestParseLanguages(t *testing.T) {
	t.Parallel()

	langs := map[string]any{
		"Go": map[string]any{
			"added":   float64(100),
			"removed": float64(20),
			"changed": float64(5),
		},
	}

	result := parseLanguages(langs)
	assert.Equal(t, 100, result["Go"].Added)
	assert.Equal(t, 20, result["Go"].Removed)
}

func TestParseLanguages_Invalid(t *testing.T) {
	t.Parallel()

	result := parseLanguages("not a map")
	assert.Empty(t, result)
}

func TestParseTickSize_Variants(t *testing.T) {
	t.Parallel()

	// Duration value.
	report := analyze.Report{"TickSize": 12 * time.Hour}
	assert.Equal(t, 12*time.Hour, parseTickSize(report))

	// Float64 value.
	report = analyze.Report{"TickSize": float64(48 * time.Hour)}
	assert.Equal(t, 48*time.Hour, parseTickSize(report))

	// Int value.
	report = analyze.Report{"TickSize": int(72 * time.Hour)}
	assert.Equal(t, 72*time.Hour, parseTickSize(report))

	// Missing key.
	report = analyze.Report{}
	assert.Equal(t, 24*time.Hour, parseTickSize(report))
}

func TestParseReversedPeopleDict_AnySlice(t *testing.T) {
	t.Parallel()

	report := analyze.Report{
		"ReversedPeopleDict": []any{"Alice", "Bob"},
	}

	names, err := parseReversedPeopleDict(report)
	require.NoError(t, err)
	assert.Equal(t, []string{"Alice", "Bob"}, names)
}

func TestParseCommitDevData_FromMap(t *testing.T) {
	t.Parallel()

	report := analyze.Report{
		"CommitDevData": map[string]any{
			"hash1": map[string]any{
				"commits":     float64(1),
				"lines_added": float64(20),
				"author_id":   float64(0),
			},
		},
	}

	result, ok := parseCommitDevData(report)
	require.True(t, ok)
	require.Len(t, result, 1)
	assert.Equal(t, 1, result["hash1"].Commits)
}

func TestParseCommitsByTick_FromMap(t *testing.T) {
	t.Parallel()

	report := analyze.Report{
		"CommitsByTick": map[string]any{
			"0": []any{testHashA},
		},
	}

	result, ok := parseCommitsByTick(report)
	require.True(t, ok)
	require.Len(t, result, 1)
}

func TestDevName_Variants(t *testing.T) {
	t.Parallel()

	names := []string{"Alice", "Bob"}

	assert.Equal(t, "Alice", devName(0, names))
	assert.Equal(t, "Bob", devName(1, names))
	assert.Contains(t, devName(99, names), "dev_99")
}

func TestRecoverMetricsFromBinary(t *testing.T) {
	t.Parallel()

	// Simulate a binary-decoded report.
	report := analyze.Report{
		"developers": []any{
			map[string]any{
				"id":            float64(0),
				"name":          "Alice",
				"commits":       float64(10),
				"lines_added":   float64(100),
				"lines_removed": float64(20),
				"lines_changed": float64(5),
				"net_lines":     float64(80),
				"languages":     map[string]any{},
				"first_tick":    float64(0),
				"last_tick":     float64(5),
				"active_ticks":  float64(3),
			},
		},
		"aggregate": map[string]any{
			"total_commits":         float64(10),
			"total_lines_added":     float64(100),
			"total_lines_removed":   float64(20),
			"total_developers":      float64(1),
			"active_developers":     float64(1),
			"analysis_period_ticks": float64(5),
			"project_bus_factor":    float64(1),
			"total_languages":       float64(1),
		},
	}

	metrics, err := recoverMetricsFromBinary(report)
	require.NoError(t, err)
	assert.Equal(t, 10, metrics.Aggregate.TotalCommits)
	assert.Len(t, metrics.Developers, 1)
}
