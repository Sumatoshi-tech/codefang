package cohesion

import (
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
)

// Test constants.
const (
	testFunctionName1  = "TestFunc1"
	testFunctionName2  = "TestFunc2"
	testFunctionName3  = "TestFunc3"
	testMessage        = "Test cohesion message"
	testTotalFunctions = 10
	testLCOM           = 0.25
	testCohesionScore  = 0.75
	testFuncCohesion   = 0.8
)

// --- ParseReportData Tests ---.

func TestParseReportData_Empty(t *testing.T) {
	t.Parallel()

	report := analyze.Report{}

	data, err := ParseReportData(report)

	require.NoError(t, err)
	require.NotNil(t, data)
	assert.Equal(t, 0, data.TotalFunctions)
	assert.InDelta(t, 0.0, data.LCOM, 0.01)
}

func TestParseReportData_Valid(t *testing.T) {
	t.Parallel()

	report := analyze.Report{
		"total_functions":   testTotalFunctions,
		"lcom":              testLCOM,
		"cohesion_score":    testCohesionScore,
		"function_cohesion": testFuncCohesion,
		"message":           testMessage,
	}

	data, err := ParseReportData(report)

	require.NoError(t, err)
	assert.Equal(t, testTotalFunctions, data.TotalFunctions)
	assert.InDelta(t, testLCOM, data.LCOM, 0.01)
	assert.InDelta(t, testCohesionScore, data.CohesionScore, 0.01)
	assert.InDelta(t, testFuncCohesion, data.FunctionCohesion, 0.01)
	assert.Equal(t, testMessage, data.Message)
}

func TestParseReportData_WithFunctions(t *testing.T) {
	t.Parallel()

	report := analyze.Report{
		"functions": []map[string]any{
			{"name": testFunctionName1, "cohesion": 0.9},
			{"name": testFunctionName2, "cohesion": 0.5},
		},
	}

	data, err := ParseReportData(report)

	require.NoError(t, err)
	require.Len(t, data.Functions, 2)
	assert.Equal(t, testFunctionName1, data.Functions[0].Name)
	assert.InDelta(t, 0.9, data.Functions[0].Cohesion, 0.01)
}

// --- FunctionCohesionMetric Tests ---.

func TestFunctionCohesionMetric_Metadata(t *testing.T) {
	t.Parallel()

	metric := NewFunctionCohesionMetric()

	assert.Equal(t, "function_cohesion", metric.Name())
	assert.Equal(t, "Function Cohesion", metric.DisplayName())
	assert.Equal(t, "list", metric.Type())
	assert.NotEmpty(t, metric.Description())
}

func TestFunctionCohesionMetric_Empty(t *testing.T) {
	t.Parallel()

	metric := NewFunctionCohesionMetric()
	input := &ReportData{Functions: nil}

	result := metric.Compute(input)

	assert.Empty(t, result)
}

func TestFunctionCohesionMetric_Compute(t *testing.T) {
	t.Parallel()

	metric := NewFunctionCohesionMetric()
	input := &ReportData{
		Functions: []FunctionData{
			{Name: testFunctionName1, Cohesion: 0.9},
			{Name: testFunctionName2, Cohesion: 0.5},
			{Name: testFunctionName3, Cohesion: 0.2},
		},
	}

	result := metric.Compute(input)

	require.Len(t, result, 3)
	// Sorted by cohesion ascending (worst first).
	assert.Equal(t, testFunctionName3, result[0].Name)
	assert.Equal(t, "Poor", result[0].QualityLevel)
	assert.Equal(t, testFunctionName2, result[1].Name)
	assert.Equal(t, "Good", result[1].QualityLevel) // 0.5 >= 0.4 = Good.
	assert.Equal(t, testFunctionName1, result[2].Name)
	assert.Equal(t, "Excellent", result[2].QualityLevel) // 0.9 >= 0.6 = Excellent.
}

// --- classifyCohesionQuality Tests ---.

func TestClassifyCohesionQuality(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name     string
		cohesion float64
		want     string
	}{
		{"Excellent - 0.9", 0.9, "Excellent"},
		{"Excellent - 0.6", 0.6, "Excellent"},
		{"Good - 0.5", 0.5, "Good"},
		{"Good - 0.4", 0.4, "Good"},
		{"Fair - 0.35", 0.35, "Fair"},
		{"Fair - 0.3", 0.3, "Fair"},
		{"Poor - 0.2", 0.2, "Poor"},
		{"Poor - 0.0", 0.0, "Poor"},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			got := classifyCohesionQuality(tt.cohesion)

			assert.Equal(t, tt.want, got)
		})
	}
}

// --- CohesionDistributionMetric Tests ---.

func TestCohesionDistributionMetric_Metadata(t *testing.T) {
	t.Parallel()

	metric := NewDistributionMetric()

	assert.Equal(t, "cohesion_distribution", metric.Name())
	assert.Equal(t, "Cohesion Distribution", metric.DisplayName())
	assert.Equal(t, "aggregate", metric.Type())
}

func TestCohesionDistributionMetric_Empty(t *testing.T) {
	t.Parallel()

	metric := NewDistributionMetric()
	input := &ReportData{Functions: nil}

	result := metric.Compute(input)

	assert.Equal(t, 0, result.Excellent)
	assert.Equal(t, 0, result.Good)
	assert.Equal(t, 0, result.Fair)
	assert.Equal(t, 0, result.Poor)
}

func TestCohesionDistributionMetric_Compute(t *testing.T) {
	t.Parallel()

	metric := NewDistributionMetric()
	input := &ReportData{
		Functions: []FunctionData{
			{Cohesion: 0.9},  // Excellent (>=0.6).
			{Cohesion: 0.7},  // Excellent (>=0.6).
			{Cohesion: 0.5},  // Good (>=0.4).
			{Cohesion: 0.4},  // Good (>=0.4).
			{Cohesion: 0.35}, // Fair (>=0.3).
			{Cohesion: 0.1},  // Poor (<0.3).
		},
	}

	result := metric.Compute(input)

	assert.Equal(t, 2, result.Excellent)
	assert.Equal(t, 2, result.Good)
	assert.Equal(t, 1, result.Fair)
	assert.Equal(t, 1, result.Poor)
}

// --- LowCohesionFunctionMetric Tests ---.

func TestLowCohesionFunctionMetric_Metadata(t *testing.T) {
	t.Parallel()

	metric := NewLowCohesionFunctionMetric()

	assert.Equal(t, "low_cohesion_functions", metric.Name())
	assert.Equal(t, "Low Cohesion Functions", metric.DisplayName())
	assert.Equal(t, "risk", metric.Type())
}

func TestLowCohesionFunctionMetric_Empty(t *testing.T) {
	t.Parallel()

	metric := NewLowCohesionFunctionMetric()
	input := &ReportData{Functions: nil}

	result := metric.Compute(input)

	assert.Empty(t, result)
}

func TestLowCohesionFunctionMetric_NoLowCohesion(t *testing.T) {
	t.Parallel()

	metric := NewLowCohesionFunctionMetric()
	input := &ReportData{
		Functions: []FunctionData{
			{Name: testFunctionName1, Cohesion: 0.9},
			{Name: testFunctionName2, Cohesion: 0.7},
		},
	}

	result := metric.Compute(input)

	assert.Empty(t, result)
}

func TestLowCohesionFunctionMetric_MediumRisk(t *testing.T) {
	t.Parallel()

	metric := NewLowCohesionFunctionMetric()
	input := &ReportData{
		Functions: []FunctionData{
			{Name: testFunctionName1, Cohesion: 0.35}, // Fair but below Good (0.4).
		},
	}

	result := metric.Compute(input)

	require.Len(t, result, 1)
	assert.Equal(t, "MEDIUM", result[0].RiskLevel)
	assert.NotEmpty(t, result[0].Recommendation)
}

func TestLowCohesionFunctionMetric_HighRisk(t *testing.T) {
	t.Parallel()

	metric := NewLowCohesionFunctionMetric()
	input := &ReportData{
		Functions: []FunctionData{
			{Name: testFunctionName1, Cohesion: 0.1}, // Poor.
		},
	}

	result := metric.Compute(input)

	require.Len(t, result, 1)
	assert.Equal(t, "HIGH", result[0].RiskLevel)
	assert.Contains(t, result[0].Recommendation, "splitting")
}

func TestLowCohesionFunctionMetric_SortedByCohesion(t *testing.T) {
	t.Parallel()

	metric := NewLowCohesionFunctionMetric()
	input := &ReportData{
		Functions: []FunctionData{
			{Name: testFunctionName1, Cohesion: 0.35},
			{Name: testFunctionName2, Cohesion: 0.1},
			{Name: testFunctionName3, Cohesion: 0.38},
		},
	}

	result := metric.Compute(input)

	require.Len(t, result, 3)
	// Sorted by cohesion ascending.
	assert.Equal(t, testFunctionName2, result[0].Name)
	assert.Equal(t, testFunctionName1, result[1].Name)
	assert.Equal(t, testFunctionName3, result[2].Name)
}

// --- CohesionAggregateMetric Tests ---.

func TestCohesionAggregateMetric_Metadata(t *testing.T) {
	t.Parallel()

	metric := NewAggregateMetric()

	assert.Equal(t, "cohesion_aggregate", metric.Name())
	assert.Equal(t, "Cohesion Summary", metric.DisplayName())
	assert.Equal(t, "aggregate", metric.Type())
}

func TestCohesionAggregateMetric_Compute(t *testing.T) {
	t.Parallel()

	metric := NewAggregateMetric()
	input := &ReportData{
		TotalFunctions:   testTotalFunctions,
		LCOM:             testLCOM,
		CohesionScore:    testCohesionScore,
		FunctionCohesion: testFuncCohesion,
		Message:          testMessage,
	}

	result := metric.Compute(input)

	assert.Equal(t, testTotalFunctions, result.TotalFunctions)
	assert.InDelta(t, testLCOM, result.LCOM, 0.01)
	assert.InDelta(t, testCohesionScore, result.CohesionScore, 0.01)
	assert.InDelta(t, testFuncCohesion, result.FunctionCohesion, 0.01)
	assert.InDelta(t, 75.0, result.HealthScore, 0.01) // 0.75 * 100
	assert.Equal(t, testMessage, result.Message)
}

// --- ComputeAllMetrics Tests ---.

func TestComputeAllMetrics_Empty(t *testing.T) {
	t.Parallel()

	report := analyze.Report{}

	result, err := ComputeAllMetrics(report)

	require.NoError(t, err)
	require.NotNil(t, result)
	assert.Empty(t, result.FunctionCohesion)
	assert.Empty(t, result.LowCohesionFunctions)
}

func TestComputeAllMetrics_Valid(t *testing.T) {
	t.Parallel()

	report := analyze.Report{
		"total_functions": testTotalFunctions,
		"cohesion_score":  testCohesionScore,
		"functions": []map[string]any{
			{"name": testFunctionName1, "cohesion": 0.9}, // Excellent (>=0.6).
			{"name": testFunctionName2, "cohesion": 0.2}, // Poor (<0.3).
		},
	}

	result, err := ComputeAllMetrics(report)

	require.NoError(t, err)
	require.NotNil(t, result)
	assert.Len(t, result.FunctionCohesion, 2)
	assert.Len(t, result.LowCohesionFunctions, 1) // 0.2 is below Good (0.4).
	assert.Equal(t, 1, result.Distribution.Excellent)
	assert.Equal(t, 1, result.Distribution.Poor)
}

// --- MetricsOutput Interface Tests ---.

func TestComputedMetrics_AnalyzerName(t *testing.T) {
	t.Parallel()

	metrics := &ComputedMetrics{}

	name := metrics.AnalyzerName()

	assert.Equal(t, "cohesion", name)
}

func TestComputedMetrics_ToJSON(t *testing.T) {
	t.Parallel()

	metrics := &ComputedMetrics{
		Aggregate: AggregateData{
			TotalFunctions: testTotalFunctions,
			CohesionScore:  testCohesionScore,
		},
	}

	result := metrics.ToJSON()

	assert.Equal(t, metrics, result)
}

func TestComputedMetrics_ToYAML(t *testing.T) {
	t.Parallel()

	metrics := &ComputedMetrics{
		Aggregate: AggregateData{
			TotalFunctions: testTotalFunctions,
			CohesionScore:  testCohesionScore,
		},
	}

	result := metrics.ToYAML()

	assert.Equal(t, metrics, result)
}
