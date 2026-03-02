package devs

import (
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/internal/identity"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

// Test constants for HLL developer cardinality tests.
const (
	// hllTestDevID0 is the first test developer ID.
	hllTestDevID0 = 0

	// hllTestDevID1 is the second test developer ID.
	hllTestDevID1 = 1

	// hllTestDevID2 is the third test developer ID.
	hllTestDevID2 = 2

	// hllTestCommits is the commit count used in HLL tests.
	hllTestCommits = 3

	// hllTestAccuracyDevs is the number of developers for accuracy testing.
	hllTestAccuracyDevs = 1000

	// hllTestAccuracyTolerance is the maximum allowed relative error (3%).
	hllTestAccuracyTolerance = 0.03

	// hllTestBenchDevs is the number of developers for benchmarking.
	hllTestBenchDevs = 5000
)

// --- devIDBytes Tests ---.

func TestDevIDBytes_Deterministic(t *testing.T) {
	t.Parallel()

	first := devIDBytes(hllTestDevID0)
	second := devIDBytes(hllTestDevID0)

	assert.Equal(t, first, second)
}

func TestDevIDBytes_NegativeID(t *testing.T) {
	t.Parallel()

	result := devIDBytes(identity.AuthorMissing)

	require.NotEmpty(t, result)
	// AuthorMissing is a large positive int (262142), should convert to bytes.
	assert.Equal(t, []byte("262142"), result)
}

func TestDevIDBytes_Unique(t *testing.T) {
	t.Parallel()

	b0 := devIDBytes(hllTestDevID0)
	b1 := devIDBytes(hllTestDevID1)
	b2 := devIDBytes(hllTestDevID2)

	assert.NotEqual(t, b0, b1)
	assert.NotEqual(t, b1, b2)
	assert.NotEqual(t, b0, b2)
}

// --- ParseTickData HLL Integration Tests ---.

func TestParseTickData_PopulatesDevSketch(t *testing.T) {
	t.Parallel()

	commitDevData := map[string]*CommitDevData{
		testHashA: {Commits: hllTestCommits, AuthorID: hllTestDevID0},
		testHashB: {Commits: hllTestCommits, AuthorID: hllTestDevID1},
	}
	commitsByTick := map[int][]gitlib.Hash{
		0: {gitlib.NewHash(testHashA)},
		1: {gitlib.NewHash(testHashB)},
	}
	names := []string{testDevName1, testDevName2}

	report := analyze.Report{
		"CommitDevData":      commitDevData,
		"CommitsByTick":      commitsByTick,
		"ReversedPeopleDict": names,
		"TickSize":           testTickSize,
	}

	data, err := ParseTickData(report)

	require.NoError(t, err)
	require.NotNil(t, data)
	require.NotNil(t, data.DevSketch, "DevSketch should be populated")

	// Two distinct developers → HLL should estimate 2.
	estimated := data.DevSketch.Count()
	assert.Equal(t, uint64(2), estimated)
}

func TestParseTickData_EmptyTicks_NilSketch(t *testing.T) {
	t.Parallel()

	report := analyze.Report{
		"ReversedPeopleDict": []string{testDevName1},
		"TickSize":           testTickSize,
	}

	data, err := ParseTickData(report)

	require.NoError(t, err)
	require.NotNil(t, data)
	assert.Nil(t, data.DevSketch, "DevSketch should be nil when no ticks")
}

// --- AggregateMetric HLL Integration Tests ---.

func TestAggregateMetric_EstimatedFields(t *testing.T) {
	t.Parallel()

	metric := NewAggregateMetric()
	input := AggregateInput{
		Developers: []DeveloperData{
			{ID: hllTestDevID0, Commits: testCommits, Added: testLinesAdded, Removed: testLinesRemoved},
			{ID: hllTestDevID1, Commits: testCommits, Added: testLinesAdded, Removed: testLinesRemoved},
		},
		Ticks: map[int]map[int]*DevTick{
			0: {hllTestDevID0: {Commits: hllTestCommits}},
			5: {hllTestDevID0: {Commits: hllTestCommits}},
			8: {hllTestDevID1: {Commits: hllTestCommits}},
		},
		TickSize: testTickSize,
	}

	result := metric.Compute(input)

	// Exact fields.
	assert.Equal(t, 2, result.TotalDevelopers)

	// HLL estimated fields.
	assert.Equal(t, uint64(2), result.EstimatedTotalDevelopers)
	assert.Equal(t, uint64(2), result.EstimatedActiveDevelopers)
}

func TestAggregateMetric_EmptyInput_HLL(t *testing.T) {
	t.Parallel()

	metric := NewAggregateMetric()
	input := AggregateInput{
		Developers: []DeveloperData{},
		Ticks:      map[int]map[int]*DevTick{},
	}

	result := metric.Compute(input)

	assert.Equal(t, uint64(0), result.EstimatedTotalDevelopers)
	assert.Equal(t, uint64(0), result.EstimatedActiveDevelopers)
}

func TestAggregateMetric_SingleDev_HLL(t *testing.T) {
	t.Parallel()

	metric := NewAggregateMetric()
	input := AggregateInput{
		Developers: []DeveloperData{
			{ID: hllTestDevID0, Commits: testCommits, Added: testLinesAdded, Removed: testLinesRemoved},
		},
		Ticks: map[int]map[int]*DevTick{
			0: {hllTestDevID0: {Commits: hllTestCommits}},
		},
		TickSize: testTickSize,
	}

	result := metric.Compute(input)

	assert.Equal(t, uint64(1), result.EstimatedTotalDevelopers)
	assert.Equal(t, uint64(1), result.EstimatedActiveDevelopers)
}

// --- HLL Accuracy Test ---.

func TestHLLAccuracy_1000Devs(t *testing.T) {
	t.Parallel()

	// Build a TickData with 1000 unique developers across multiple ticks.
	ticks := make(map[int]map[int]*DevTick)

	for i := range hllTestAccuracyDevs {
		tick := i % testCommits

		if ticks[tick] == nil {
			ticks[tick] = make(map[int]*DevTick)
		}

		ticks[tick][i] = &DevTick{Commits: 1}
	}

	metric := NewAggregateMetric()
	names := make([]string, hllTestAccuracyDevs)

	for i := range hllTestAccuracyDevs {
		names[i] = "dev"
	}

	developers := make([]DeveloperData, hllTestAccuracyDevs)

	for i := range hllTestAccuracyDevs {
		developers[i] = DeveloperData{ID: i, Commits: 1, Added: 1, Removed: 0}
	}

	input := AggregateInput{
		Developers: developers,
		Ticks:      ticks,
		TickSize:   testTickSize,
	}

	result := metric.Compute(input)

	// HLL should estimate within 3% of exact 1000.
	estimated := float64(result.EstimatedTotalDevelopers)
	exact := float64(hllTestAccuracyDevs)
	relativeError := abs64(estimated-exact) / exact

	assert.LessOrEqual(t, relativeError, hllTestAccuracyTolerance,
		"HLL estimate %d should be within 3%% of exact %d (error: %.4f)",
		result.EstimatedTotalDevelopers, hllTestAccuracyDevs, relativeError)
}

// abs64 returns the absolute value of a float64.
func abs64(x float64) float64 {
	if x < 0 {
		return -x
	}

	return x
}

// --- Benchmarks ---.

func BenchmarkDevCardinality_HLL(b *testing.B) {
	ticks := make(map[int]map[int]*DevTick)

	for i := range hllTestBenchDevs {
		tick := i % testCommits

		if ticks[tick] == nil {
			ticks[tick] = make(map[int]*DevTick)
		}

		ticks[tick][i] = &DevTick{Commits: 1}
	}

	developers := make([]DeveloperData, hllTestBenchDevs)

	for i := range hllTestBenchDevs {
		developers[i] = DeveloperData{ID: i, Commits: 1, Added: 1}
	}

	input := AggregateInput{
		Developers: developers,
		Ticks:      ticks,
		TickSize:   testTickSize,
	}
	metric := NewAggregateMetric()

	b.ResetTimer()

	for range b.N {
		result := metric.Compute(input)
		_ = result.EstimatedTotalDevelopers
	}
}

func BenchmarkDevCardinality_Exact(b *testing.B) {
	ticks := make(map[int]map[int]*DevTick)

	for i := range hllTestBenchDevs {
		tick := i % testCommits

		if ticks[tick] == nil {
			ticks[tick] = make(map[int]*DevTick)
		}

		ticks[tick][i] = &DevTick{Commits: 1}
	}

	developers := make([]DeveloperData, hllTestBenchDevs)

	for i := range hllTestBenchDevs {
		developers[i] = DeveloperData{ID: i, Commits: 1, Added: 1}
	}

	input := AggregateInput{
		Developers: developers,
		Ticks:      ticks,
		TickSize:   testTickSize,
	}
	metric := NewAggregateMetric()

	b.ResetTimer()

	for range b.N {
		result := metric.Compute(input)
		_ = result.TotalDevelopers
	}
}
