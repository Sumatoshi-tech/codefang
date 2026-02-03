package halstead

import (
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
)

// Test constants to avoid magic strings/numbers.
const (
	testFunctionName1 = "TestFunc1"
	testFunctionName2 = "TestFunc2"
	testFunctionName3 = "TestFunc3"
	testMessage       = "Test halstead message"

	testVolumeLow      = 50.0
	testVolumeMedium   = 500.0
	testVolumeHigh     = 3000.0
	testVolumeVeryHigh = 6000.0

	testDifficulty    = 15.5
	testEffort        = 1000.0
	testTimeToProgram = 55.5
	testDeliveredBugs = 0.5

	testDistinctOperators = 25
	testDistinctOperands  = 50
	testTotalOperators    = 100
	testTotalOperands     = 200
	testVocabulary        = 75
	testLength            = 300
	testEstimatedLength   = 280.5
	testTotalFunctions    = 10

	floatDelta = 0.01
)

// --- ParseReportData Tests ---

func TestParseReportData_Empty(t *testing.T) {
	report := analyze.Report{}

	result, err := ParseReportData(report)

	require.NoError(t, err)
	assert.Equal(t, 0, result.TotalFunctions)
	assert.Empty(t, result.Functions)
	assert.Empty(t, result.Message)
}

func TestParseReportData_AllFields(t *testing.T) {
	report := analyze.Report{
		"total_functions":    testTotalFunctions,
		"volume":             testVolumeHigh,
		"difficulty":         testDifficulty,
		"effort":             testEffort,
		"time_to_program":    testTimeToProgram,
		"delivered_bugs":     testDeliveredBugs,
		"distinct_operators": testDistinctOperators,
		"distinct_operands":  testDistinctOperands,
		"total_operators":    testTotalOperators,
		"total_operands":     testTotalOperands,
		"vocabulary":         testVocabulary,
		"length":             testLength,
		"estimated_length":   testEstimatedLength,
		"message":            testMessage,
	}

	result, err := ParseReportData(report)

	require.NoError(t, err)
	assert.Equal(t, testTotalFunctions, result.TotalFunctions)
	assert.InDelta(t, testVolumeHigh, result.Volume, floatDelta)
	assert.InDelta(t, testDifficulty, result.Difficulty, floatDelta)
	assert.InDelta(t, testEffort, result.Effort, floatDelta)
	assert.InDelta(t, testTimeToProgram, result.TimeToProgram, floatDelta)
	assert.InDelta(t, testDeliveredBugs, result.DeliveredBugs, floatDelta)
	assert.Equal(t, testDistinctOperators, result.DistinctOperators)
	assert.Equal(t, testDistinctOperands, result.DistinctOperands)
	assert.Equal(t, testTotalOperators, result.TotalOperators)
	assert.Equal(t, testTotalOperands, result.TotalOperands)
	assert.Equal(t, testVocabulary, result.Vocabulary)
	assert.Equal(t, testLength, result.Length)
	assert.InDelta(t, testEstimatedLength, result.EstimatedLength, floatDelta)
	assert.Equal(t, testMessage, result.Message)
}

func TestParseReportData_WithFunctions(t *testing.T) {
	report := analyze.Report{
		"functions": []map[string]any{
			{
				"name":               testFunctionName1,
				"volume":             testVolumeMedium,
				"difficulty":         testDifficulty,
				"effort":             testEffort,
				"time_to_program":    testTimeToProgram,
				"delivered_bugs":     testDeliveredBugs,
				"distinct_operators": testDistinctOperators,
				"distinct_operands":  testDistinctOperands,
				"total_operators":    testTotalOperators,
				"total_operands":     testTotalOperands,
				"vocabulary":         testVocabulary,
				"length":             testLength,
			},
			{
				"name":   testFunctionName2,
				"volume": testVolumeHigh,
			},
		},
	}

	result, err := ParseReportData(report)

	require.NoError(t, err)
	require.Len(t, result.Functions, 2)

	// First function - all fields
	fn1 := result.Functions[0]
	assert.Equal(t, testFunctionName1, fn1.Name)
	assert.InDelta(t, testVolumeMedium, fn1.Volume, floatDelta)
	assert.InDelta(t, testDifficulty, fn1.Difficulty, floatDelta)
	assert.InDelta(t, testEffort, fn1.Effort, floatDelta)
	assert.InDelta(t, testTimeToProgram, fn1.TimeToProgram, floatDelta)
	assert.InDelta(t, testDeliveredBugs, fn1.DeliveredBugs, floatDelta)
	assert.Equal(t, testDistinctOperators, fn1.DistinctOperators)
	assert.Equal(t, testDistinctOperands, fn1.DistinctOperands)
	assert.Equal(t, testTotalOperators, fn1.TotalOperators)
	assert.Equal(t, testTotalOperands, fn1.TotalOperands)
	assert.Equal(t, testVocabulary, fn1.Vocabulary)
	assert.Equal(t, testLength, fn1.Length)

	// Second function - minimal fields
	fn2 := result.Functions[1]
	assert.Equal(t, testFunctionName2, fn2.Name)
	assert.InDelta(t, testVolumeHigh, fn2.Volume, floatDelta)
}

// --- FunctionHalsteadMetric Tests ---

func TestNewFunctionHalsteadMetric_Metadata(t *testing.T) {
	m := NewFunctionHalsteadMetric()

	assert.Equal(t, "function_halstead", m.Name())
	assert.Equal(t, "Function Halstead Metrics", m.DisplayName())
	assert.Contains(t, m.Description(), "Per-function Halstead complexity")
	assert.Equal(t, "list", m.Type())
}

func TestFunctionHalsteadMetric_Empty(t *testing.T) {
	m := NewFunctionHalsteadMetric()
	input := &ReportData{}

	result := m.Compute(input)

	assert.Empty(t, result)
}

func TestFunctionHalsteadMetric_SingleFunction(t *testing.T) {
	m := NewFunctionHalsteadMetric()
	input := &ReportData{
		Functions: []FunctionData{
			{
				Name:          testFunctionName1,
				Volume:        testVolumeMedium,
				Difficulty:    testDifficulty,
				Effort:        testEffort,
				TimeToProgram: testTimeToProgram,
				DeliveredBugs: testDeliveredBugs,
			},
		},
	}

	result := m.Compute(input)

	require.Len(t, result, 1)
	assert.Equal(t, testFunctionName1, result[0].Name)
	assert.InDelta(t, testVolumeMedium, result[0].Volume, floatDelta)
	assert.InDelta(t, testDifficulty, result[0].Difficulty, floatDelta)
	assert.InDelta(t, testEffort, result[0].Effort, floatDelta)
	assert.InDelta(t, testTimeToProgram, result[0].TimeToProgram, floatDelta)
	assert.InDelta(t, testDeliveredBugs, result[0].DeliveredBugs, floatDelta)
	assert.Equal(t, "Medium", result[0].ComplexityLevel)
}

func TestFunctionHalsteadMetric_SortedByVolume(t *testing.T) {
	m := NewFunctionHalsteadMetric()
	input := &ReportData{
		Functions: []FunctionData{
			{Name: testFunctionName1, Volume: testVolumeLow},
			{Name: testFunctionName2, Volume: testVolumeVeryHigh},
			{Name: testFunctionName3, Volume: testVolumeMedium},
		},
	}

	result := m.Compute(input)

	require.Len(t, result, 3)
	// Should be sorted by volume descending
	assert.Equal(t, testFunctionName2, result[0].Name)
	assert.Equal(t, testFunctionName3, result[1].Name)
	assert.Equal(t, testFunctionName1, result[2].Name)
}

func TestFunctionHalsteadMetric_ComplexityLevels(t *testing.T) {
	tests := []struct {
		name     string
		volume   float64
		expected string
	}{
		{"very_high", 6000.0, "Very High"},
		{"high_boundary", 5000.0, "Very High"},
		{"high", 3000.0, "High"},
		{"medium_boundary", 1000.0, "High"},
		{"medium", 500.0, "Medium"},
		{"low_boundary", 100.0, "Medium"},
		{"low", 50.0, "Low"},
		{"zero", 0.0, "Low"},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			m := NewFunctionHalsteadMetric()
			input := &ReportData{
				Functions: []FunctionData{
					{Name: testFunctionName1, Volume: tt.volume},
				},
			}

			result := m.Compute(input)

			require.Len(t, result, 1)
			assert.Equal(t, tt.expected, result[0].ComplexityLevel)
		})
	}
}

// --- classifyVolumeLevel Tests ---

func TestClassifyVolumeLevel(t *testing.T) {
	tests := []struct {
		name     string
		volume   float64
		expected string
	}{
		{"very_high", testVolumeVeryHigh, "Very High"},
		{"high_boundary", VolumeThresholdHigh, "Very High"},
		{"high", testVolumeHigh, "High"},
		{"medium_boundary", VolumeThresholdMedium, "High"},
		{"medium", testVolumeMedium, "Medium"},
		{"low_boundary", VolumeThresholdLow, "Medium"},
		{"low", testVolumeLow, "Low"},
		{"zero", 0, "Low"},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := classifyVolumeLevel(tt.volume)
			assert.Equal(t, tt.expected, result)
		})
	}
}

// --- EffortDistributionMetric Tests ---

func TestNewEffortDistributionMetric_Metadata(t *testing.T) {
	m := NewEffortDistributionMetric()

	assert.Equal(t, "effort_distribution", m.Name())
	assert.Equal(t, "Effort Distribution", m.DisplayName())
	assert.Contains(t, m.Description(), "Distribution of functions")
	assert.Equal(t, "aggregate", m.Type())
}

func TestEffortDistributionMetric_Empty(t *testing.T) {
	m := NewEffortDistributionMetric()
	input := &ReportData{}

	result := m.Compute(input)

	assert.Equal(t, 0, result.Low)
	assert.Equal(t, 0, result.Medium)
	assert.Equal(t, 0, result.High)
	assert.Equal(t, 0, result.VeryHigh)
}

func TestEffortDistributionMetric_AllLevels(t *testing.T) {
	m := NewEffortDistributionMetric()
	input := &ReportData{
		Functions: []FunctionData{
			{Name: "low1", Volume: 50},
			{Name: "low2", Volume: 99},
			{Name: "medium1", Volume: 100},
			{Name: "medium2", Volume: 500},
			{Name: "high1", Volume: 1000},
			{Name: "high2", Volume: 2000},
			{Name: "veryhigh1", Volume: 5000},
			{Name: "veryhigh2", Volume: 10000},
		},
	}

	result := m.Compute(input)

	assert.Equal(t, 2, result.Low)
	assert.Equal(t, 2, result.Medium)
	assert.Equal(t, 2, result.High)
	assert.Equal(t, 2, result.VeryHigh)
}

func TestEffortDistributionMetric_SingleCategory(t *testing.T) {
	m := NewEffortDistributionMetric()
	input := &ReportData{
		Functions: []FunctionData{
			{Name: "low1", Volume: 10},
			{Name: "low2", Volume: 20},
			{Name: "low3", Volume: 30},
		},
	}

	result := m.Compute(input)

	assert.Equal(t, 3, result.Low)
	assert.Equal(t, 0, result.Medium)
	assert.Equal(t, 0, result.High)
	assert.Equal(t, 0, result.VeryHigh)
}

// --- HighEffortFunctionMetric Tests ---

func TestNewHighEffortFunctionMetric_Metadata(t *testing.T) {
	m := NewHighEffortFunctionMetric()

	assert.Equal(t, "high_effort_functions", m.Name())
	assert.Equal(t, "High Effort Functions", m.DisplayName())
	assert.Contains(t, m.Description(), "high Halstead effort")
	assert.Equal(t, "risk", m.Type())
}

func TestHighEffortFunctionMetric_Empty(t *testing.T) {
	m := NewHighEffortFunctionMetric()
	input := &ReportData{}

	result := m.Compute(input)

	assert.Empty(t, result)
}

func TestHighEffortFunctionMetric_BelowThreshold(t *testing.T) {
	m := NewHighEffortFunctionMetric()
	input := &ReportData{
		Functions: []FunctionData{
			{Name: "low1", Volume: 50},
			{Name: "low2", Volume: 99},
			{Name: "low3", Volume: 999},
		},
	}

	result := m.Compute(input)

	assert.Empty(t, result)
}

func TestHighEffortFunctionMetric_MediumRisk(t *testing.T) {
	m := NewHighEffortFunctionMetric()
	input := &ReportData{
		Functions: []FunctionData{
			{
				Name:          testFunctionName1,
				Volume:        testVolumeHigh,
				Effort:        testEffort,
				TimeToProgram: testTimeToProgram,
				DeliveredBugs: testDeliveredBugs,
			},
		},
	}

	result := m.Compute(input)

	require.Len(t, result, 1)
	assert.Equal(t, testFunctionName1, result[0].Name)
	assert.InDelta(t, testVolumeHigh, result[0].Volume, floatDelta)
	assert.InDelta(t, testEffort, result[0].Effort, floatDelta)
	assert.InDelta(t, testTimeToProgram, result[0].TimeToProgram, floatDelta)
	assert.InDelta(t, testDeliveredBugs, result[0].DeliveredBugs, floatDelta)
	assert.Equal(t, "MEDIUM", result[0].RiskLevel)
}

func TestHighEffortFunctionMetric_HighRisk(t *testing.T) {
	m := NewHighEffortFunctionMetric()
	input := &ReportData{
		Functions: []FunctionData{
			{Name: testFunctionName1, Volume: testVolumeVeryHigh},
		},
	}

	result := m.Compute(input)

	require.Len(t, result, 1)
	assert.Equal(t, "HIGH", result[0].RiskLevel)
}

func TestHighEffortFunctionMetric_RiskLevelThreshold(t *testing.T) {
	tests := []struct {
		name     string
		volume   float64
		expected string
	}{
		{"high_risk", 5000.0, "HIGH"},
		{"high_risk_above", 6000.0, "HIGH"},
		{"medium_risk", 4999.0, "MEDIUM"},
		{"medium_risk_boundary", 1000.0, "MEDIUM"},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			m := NewHighEffortFunctionMetric()
			input := &ReportData{
				Functions: []FunctionData{
					{Name: testFunctionName1, Volume: tt.volume},
				},
			}

			result := m.Compute(input)

			require.Len(t, result, 1)
			assert.Equal(t, tt.expected, result[0].RiskLevel)
		})
	}
}

func TestHighEffortFunctionMetric_SortedByVolume(t *testing.T) {
	m := NewHighEffortFunctionMetric()
	input := &ReportData{
		Functions: []FunctionData{
			{Name: testFunctionName1, Volume: 1500},
			{Name: testFunctionName2, Volume: 6000},
			{Name: testFunctionName3, Volume: 3000},
		},
	}

	result := m.Compute(input)

	require.Len(t, result, 3)
	// Should be sorted by volume descending
	assert.Equal(t, testFunctionName2, result[0].Name)
	assert.Equal(t, testFunctionName3, result[1].Name)
	assert.Equal(t, testFunctionName1, result[2].Name)
}

// --- HalsteadAggregateMetric Tests ---

func TestNewAggregateMetric_Metadata(t *testing.T) {
	m := NewAggregateMetric()

	assert.Equal(t, "halstead_aggregate", m.Name())
	assert.Equal(t, "Halstead Summary", m.DisplayName())
	assert.Contains(t, m.Description(), "Aggregate Halstead metrics")
	assert.Equal(t, "aggregate", m.Type())
}

func TestHalsteadAggregateMetric_Empty(t *testing.T) {
	m := NewAggregateMetric()
	input := &ReportData{}

	result := m.Compute(input)

	assert.Equal(t, 0, result.TotalFunctions)
	assert.InDelta(t, 0.0, result.Volume, floatDelta)
	assert.InDelta(t, 0.0, result.HealthScore, floatDelta)
}

func TestHalsteadAggregateMetric_AllFields(t *testing.T) {
	m := NewAggregateMetric()
	input := &ReportData{
		TotalFunctions:    testTotalFunctions,
		Volume:            testVolumeHigh,
		Difficulty:        testDifficulty,
		Effort:            testEffort,
		TimeToProgram:     testTimeToProgram,
		DeliveredBugs:     testDeliveredBugs,
		DistinctOperators: testDistinctOperators,
		DistinctOperands:  testDistinctOperands,
		TotalOperators:    testTotalOperators,
		TotalOperands:     testTotalOperands,
		Vocabulary:        testVocabulary,
		Length:            testLength,
		EstimatedLength:   testEstimatedLength,
		Message:           testMessage,
	}

	result := m.Compute(input)

	assert.Equal(t, testTotalFunctions, result.TotalFunctions)
	assert.InDelta(t, testVolumeHigh, result.Volume, floatDelta)
	assert.InDelta(t, testDifficulty, result.Difficulty, floatDelta)
	assert.InDelta(t, testEffort, result.Effort, floatDelta)
	assert.InDelta(t, testTimeToProgram, result.TimeToProgram, floatDelta)
	assert.InDelta(t, testDeliveredBugs, result.DeliveredBugs, floatDelta)
	assert.Equal(t, testDistinctOperators, result.DistinctOperators)
	assert.Equal(t, testDistinctOperands, result.DistinctOperands)
	assert.Equal(t, testTotalOperators, result.TotalOperators)
	assert.Equal(t, testTotalOperands, result.TotalOperands)
	assert.Equal(t, testVocabulary, result.Vocabulary)
	assert.Equal(t, testLength, result.Length)
	assert.InDelta(t, testEstimatedLength, result.EstimatedLength, floatDelta)
	assert.Equal(t, testMessage, result.Message)
}

func TestHalsteadAggregateMetric_HealthScoreLevels(t *testing.T) {
	tests := []struct {
		name           string
		totalFunctions int
		volume         float64
		minHealth      float64
		maxHealth      float64
	}{
		{"perfect_low_volume", 10, 500.0, 99.0, 100.0},
		{"high_medium_volume", 10, 2000.0, 70.0, 100.0},
		{"medium_high_volume", 10, 20000.0, 30.0, 70.0},
		{"low_very_high_volume", 10, 100000.0, 0.0, 30.0},
		{"zero_functions", 0, testVolumeHigh, 0.0, floatDelta},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			m := NewAggregateMetric()
			input := &ReportData{
				TotalFunctions: tt.totalFunctions,
				Volume:         tt.volume,
			}

			result := m.Compute(input)

			assert.GreaterOrEqual(t, result.HealthScore, tt.minHealth)
			assert.LessOrEqual(t, result.HealthScore, tt.maxHealth)
		})
	}
}

func TestHalsteadAggregateMetric_HealthScoreNeverNegative(t *testing.T) {
	m := NewAggregateMetric()
	input := &ReportData{
		TotalFunctions: 1,
		Volume:         1000000.0, // Extremely high volume
	}

	result := m.Compute(input)

	assert.GreaterOrEqual(t, result.HealthScore, 0.0)
}

// --- ComputeAllMetrics Tests ---

func TestComputeAllMetrics_Empty(t *testing.T) {
	report := analyze.Report{}

	result, err := ComputeAllMetrics(report)

	require.NoError(t, err)
	assert.Empty(t, result.FunctionHalstead)
	assert.Empty(t, result.HighEffortFunctions)
	assert.Equal(t, 0, result.Distribution.Low)
	assert.Equal(t, 0, result.Aggregate.TotalFunctions)
}

func TestComputeAllMetrics_Full(t *testing.T) {
	report := analyze.Report{
		"total_functions":    3,
		"volume":             9050.0,
		"difficulty":         testDifficulty,
		"effort":             testEffort,
		"time_to_program":    testTimeToProgram,
		"delivered_bugs":     testDeliveredBugs,
		"distinct_operators": testDistinctOperators,
		"distinct_operands":  testDistinctOperands,
		"total_operators":    testTotalOperators,
		"total_operands":     testTotalOperands,
		"vocabulary":         testVocabulary,
		"length":             testLength,
		"estimated_length":   testEstimatedLength,
		"message":            testMessage,
		"functions": []map[string]any{
			{"name": "lowFunc", "volume": testVolumeLow},
			{"name": "highFunc", "volume": testVolumeHigh},
			{"name": "veryHighFunc", "volume": testVolumeVeryHigh},
		},
	}

	result, err := ComputeAllMetrics(report)

	require.NoError(t, err)

	// FunctionHalstead - sorted by volume descending
	require.Len(t, result.FunctionHalstead, 3)
	assert.Equal(t, "veryHighFunc", result.FunctionHalstead[0].Name)
	assert.Equal(t, "highFunc", result.FunctionHalstead[1].Name)
	assert.Equal(t, "lowFunc", result.FunctionHalstead[2].Name)

	// Distribution
	assert.Equal(t, 1, result.Distribution.Low)
	assert.Equal(t, 0, result.Distribution.Medium)
	assert.Equal(t, 1, result.Distribution.High)
	assert.Equal(t, 1, result.Distribution.VeryHigh)

	// HighEffortFunctions - only High and VeryHigh volume functions
	require.Len(t, result.HighEffortFunctions, 2)
	assert.Equal(t, "veryHighFunc", result.HighEffortFunctions[0].Name)
	assert.Equal(t, "HIGH", result.HighEffortFunctions[0].RiskLevel)
	assert.Equal(t, "highFunc", result.HighEffortFunctions[1].Name)
	assert.Equal(t, "MEDIUM", result.HighEffortFunctions[1].RiskLevel)

	// Aggregate
	assert.Equal(t, 3, result.Aggregate.TotalFunctions)
	assert.InDelta(t, 9050.0, result.Aggregate.Volume, floatDelta)
	assert.Equal(t, testMessage, result.Aggregate.Message)
	assert.Greater(t, result.Aggregate.HealthScore, 0.0)
}

// --- MetricsOutput Interface Tests ---

func TestComputedMetrics_AnalyzerName(t *testing.T) {
	m := &ComputedMetrics{}

	assert.Equal(t, "halstead", m.AnalyzerName())
}

func TestComputedMetrics_ToJSON(t *testing.T) {
	m := &ComputedMetrics{
		FunctionHalstead: []FunctionHalsteadData{
			{Name: testFunctionName1, Volume: testVolumeMedium},
		},
		Distribution: EffortDistributionData{Low: 1},
		Aggregate:    AggregateData{TotalFunctions: 1},
	}

	result := m.ToJSON()

	assert.Equal(t, m, result)
}

func TestComputedMetrics_ToYAML(t *testing.T) {
	m := &ComputedMetrics{
		FunctionHalstead: []FunctionHalsteadData{
			{Name: testFunctionName1, Volume: testVolumeMedium},
		},
		Distribution: EffortDistributionData{Low: 1},
		Aggregate:    AggregateData{TotalFunctions: 1},
	}

	result := m.ToYAML()

	assert.Equal(t, m, result)
}
