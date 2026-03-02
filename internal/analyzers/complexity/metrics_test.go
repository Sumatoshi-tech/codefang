package complexity

import (
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
)

// Test constants to avoid magic strings/numbers.
const (
	testFunctionName        = "TestFunction"
	testFunctionName2       = "AnotherFunction"
	testFunctionName3       = "ThirdFunction"
	testMessage             = "Test complexity message"
	testLinesOfCode         = 100
	testLinesOfCodeZero     = 0
	testTotalFunctions      = 10
	testAverageComplexity   = 5.5
	testMaxComplexity       = 15
	testTotalComplexity     = 55
	testCognitiveComplexity = 25
	testNestingDepth        = 3
	testDecisionPoints      = 20
)

func TestParseReportData_Empty(t *testing.T) {
	t.Parallel()

	report := analyze.Report{}

	data, err := ParseReportData(report)

	require.NoError(t, err)
	require.NotNil(t, data)
	assert.Equal(t, 0, data.TotalFunctions)
	assert.InDelta(t, 0.0, data.AverageComplexity, 0.001)
	assert.Empty(t, data.Functions)
}

func TestParseReportData_ValidInput(t *testing.T) {
	t.Parallel()

	report := analyze.Report{
		"total_functions":      testTotalFunctions,
		"average_complexity":   testAverageComplexity,
		"max_complexity":       testMaxComplexity,
		"total_complexity":     testTotalComplexity,
		"cognitive_complexity": testCognitiveComplexity,
		"nesting_depth":        testNestingDepth,
		"decision_points":      testDecisionPoints,
		"message":              testMessage,
	}

	data, err := ParseReportData(report)

	require.NoError(t, err)
	require.NotNil(t, data)
	assert.Equal(t, testTotalFunctions, data.TotalFunctions)
	assert.InDelta(t, testAverageComplexity, data.AverageComplexity, 0.001)
	assert.Equal(t, testMaxComplexity, data.MaxComplexity)
	assert.Equal(t, testTotalComplexity, data.TotalComplexity)
	assert.Equal(t, testCognitiveComplexity, data.CognitiveComplexity)
	assert.Equal(t, testNestingDepth, data.NestingDepth)
	assert.Equal(t, testDecisionPoints, data.DecisionPoints)
	assert.Equal(t, testMessage, data.Message)
}

func TestParseReportData_WithFunctions(t *testing.T) {
	t.Parallel()

	report := analyze.Report{
		"functions": []map[string]any{
			{
				"name":                  testFunctionName,
				"cyclomatic_complexity": CyclomaticThresholdHigh,
				"cognitive_complexity":  CognitiveThresholdModerate,
				"nesting_depth":         testNestingDepth,
				"lines_of_code":         testLinesOfCode,
			},
		},
	}

	data, err := ParseReportData(report)

	require.NoError(t, err)
	require.Len(t, data.Functions, 1)
	assert.Equal(t, testFunctionName, data.Functions[0].Name)
	assert.Equal(t, CyclomaticThresholdHigh, data.Functions[0].CyclomaticComplexity)
	assert.Equal(t, CognitiveThresholdModerate, data.Functions[0].CognitiveComplexity)
	assert.Equal(t, testNestingDepth, data.Functions[0].NestingDepth)
	assert.Equal(t, testLinesOfCode, data.Functions[0].LinesOfCode)
}

func TestParseReportData_WithAssessments(t *testing.T) {
	t.Parallel()

	report := analyze.Report{
		"functions": []map[string]any{
			{
				"name":                  testFunctionName,
				"complexity_assessment": "high",
				"cognitive_assessment":  "moderate",
				"nesting_assessment":    "low",
			},
		},
	}

	data, err := ParseReportData(report)

	require.NoError(t, err)
	require.Len(t, data.Functions, 1)
	assert.Equal(t, "high", data.Functions[0].ComplexityAssessment)
	assert.Equal(t, "moderate", data.Functions[0].CognitiveAssessment)
	assert.Equal(t, "low", data.Functions[0].NestingAssessment)
}

// Helper to create test ReportData with functions.
func makeTestReportData(functions []FunctionData) *ReportData {
	return &ReportData{
		TotalFunctions:      len(functions),
		AverageComplexity:   testAverageComplexity,
		MaxComplexity:       testMaxComplexity,
		TotalComplexity:     testTotalComplexity,
		CognitiveComplexity: testCognitiveComplexity,
		NestingDepth:        testNestingDepth,
		DecisionPoints:      testDecisionPoints,
		Functions:           functions,
		Message:             testMessage,
	}
}

func TestClassifyFunctionRisk(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name       string
		cyclomatic int
		cognitive  int
		nesting    int
		want       string
	}{
		{
			name:       "LOW - all below moderate",
			cyclomatic: CyclomaticThresholdModerate - 1,
			cognitive:  CognitiveThresholdModerate - 1,
			nesting:    NestingThresholdModerate - 1,
			want:       "LOW",
		},
		{
			name:       "MEDIUM - one at moderate",
			cyclomatic: CyclomaticThresholdModerate,
			cognitive:  0,
			nesting:    0,
			want:       "MEDIUM",
		},
		{
			name:       "MEDIUM - one at high (score=2)",
			cyclomatic: CyclomaticThresholdHigh,
			cognitive:  0,
			nesting:    0,
			want:       "MEDIUM",
		},
		{
			name:       "HIGH - multiple moderate",
			cyclomatic: CyclomaticThresholdModerate,
			cognitive:  CognitiveThresholdModerate,
			nesting:    NestingThresholdModerate,
			want:       "HIGH",
		},
		{
			name:       "CRITICAL - all high",
			cyclomatic: CyclomaticThresholdHigh,
			cognitive:  CognitiveThresholdHigh,
			nesting:    NestingThresholdHigh,
			want:       "CRITICAL",
		},
		{
			name:       "CRITICAL - mixed high scores",
			cyclomatic: CyclomaticThresholdHigh,
			cognitive:  CognitiveThresholdHigh,
			nesting:    NestingThresholdModerate,
			want:       "CRITICAL",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			got := classifyFunctionRisk(tt.cyclomatic, tt.cognitive, tt.nesting)

			assert.Equal(t, tt.want, got)
		})
	}
}

func TestFunctionComplexityMetric_Metadata(t *testing.T) {
	t.Parallel()

	metric := NewFunctionComplexityMetric()

	assert.Equal(t, "function_complexity", metric.Name())
	assert.Equal(t, "Function Complexity", metric.DisplayName())
	assert.Equal(t, "list", metric.Type())
	assert.NotEmpty(t, metric.Description())
}

func TestFunctionComplexityMetric_Compute_Empty(t *testing.T) {
	t.Parallel()

	metric := NewFunctionComplexityMetric()
	input := makeTestReportData(nil)

	result := metric.Compute(input)

	assert.Empty(t, result)
}

func TestFunctionComplexityMetric_Compute(t *testing.T) {
	t.Parallel()

	functions := []FunctionData{
		{Name: testFunctionName, CyclomaticComplexity: 5, LinesOfCode: testLinesOfCode},
		{Name: testFunctionName2, CyclomaticComplexity: 15, LinesOfCode: testLinesOfCode},
		{Name: testFunctionName3, CyclomaticComplexity: 3, LinesOfCode: testLinesOfCode},
	}
	metric := NewFunctionComplexityMetric()
	input := makeTestReportData(functions)

	result := metric.Compute(input)

	require.Len(t, result, 3)
	// Should be sorted by cyclomatic complexity descending.
	assert.Equal(t, testFunctionName2, result[0].Name)
	assert.Equal(t, 15, result[0].CyclomaticComplexity)
	assert.Equal(t, testFunctionName, result[1].Name)
	assert.Equal(t, testFunctionName3, result[2].Name)
}

func TestFunctionComplexityMetric_ComplexityDensity(t *testing.T) {
	t.Parallel()

	functions := []FunctionData{
		{Name: testFunctionName, CyclomaticComplexity: 10, LinesOfCode: testLinesOfCode},
		{Name: testFunctionName2, CyclomaticComplexity: 5, LinesOfCode: testLinesOfCodeZero},
	}
	metric := NewFunctionComplexityMetric()
	input := makeTestReportData(functions)

	result := metric.Compute(input)

	// Density is calculated as complexity divided by lines of code.
	assert.InDelta(t, 0.1, result[0].ComplexityDensity, 0.001)
	assert.InDelta(t, 0.0, result[1].ComplexityDensity, 0.001)
}

// --- ComplexityDistributionMetric Tests ---.

func TestComplexityDistributionMetric_Metadata(t *testing.T) {
	t.Parallel()

	metric := NewDistributionMetric()

	assert.Equal(t, "complexity_distribution", metric.Name())
	assert.Equal(t, "Complexity Distribution", metric.DisplayName())
	assert.Equal(t, "aggregate", metric.Type())
	assert.NotEmpty(t, metric.Description())
}

func TestComplexityDistributionMetric_Empty(t *testing.T) {
	t.Parallel()

	metric := NewDistributionMetric()
	input := makeTestReportData(nil)

	result := metric.Compute(input)

	assert.Equal(t, 0, result.Simple)
	assert.Equal(t, 0, result.Moderate)
	assert.Equal(t, 0, result.Complex)
}

func TestComplexityDistributionMetric_Compute(t *testing.T) {
	t.Parallel()

	functions := []FunctionData{
		{Name: "simple1", CyclomaticComplexity: 1},    // Simple (<=5).
		{Name: "simple2", CyclomaticComplexity: 5},    // Simple (<=5).
		{Name: "moderate1", CyclomaticComplexity: 6},  // Moderate (6-10).
		{Name: "moderate2", CyclomaticComplexity: 10}, // Moderate (6-10).
		{Name: "complex1", CyclomaticComplexity: 11},  // Complex (>10).
		{Name: "complex2", CyclomaticComplexity: 20},  // Complex (>10).
	}
	metric := NewDistributionMetric()
	input := makeTestReportData(functions)

	result := metric.Compute(input)

	assert.Equal(t, 2, result.Simple)
	assert.Equal(t, 2, result.Moderate)
	assert.Equal(t, 2, result.Complex)
}

// --- HighRiskFunctionMetric Tests ---.

func TestHighRiskFunctionMetric_Metadata(t *testing.T) {
	t.Parallel()

	metric := NewHighRiskFunctionMetric()

	assert.Equal(t, "high_risk_functions", metric.Name())
	assert.Equal(t, "High Risk Functions", metric.DisplayName())
	assert.Equal(t, "risk", metric.Type())
	assert.NotEmpty(t, metric.Description())
}

func TestHighRiskFunctionMetric_Empty(t *testing.T) {
	t.Parallel()

	metric := NewHighRiskFunctionMetric()
	input := makeTestReportData(nil)

	result := metric.Compute(input)

	assert.Empty(t, result)
}

func TestHighRiskFunctionMetric_NoHighRisk(t *testing.T) {
	t.Parallel()

	// Functions below all high thresholds.
	functions := []FunctionData{
		{
			Name:                 testFunctionName,
			CyclomaticComplexity: CyclomaticThresholdHigh - 1,
			CognitiveComplexity:  CognitiveThresholdHigh - 1,
			NestingDepth:         NestingThresholdHigh - 1,
		},
	}
	metric := NewHighRiskFunctionMetric()
	input := makeTestReportData(functions)

	result := metric.Compute(input)

	assert.Empty(t, result)
}

func TestHighRiskFunctionMetric_HighCyclomatic(t *testing.T) {
	t.Parallel()

	functions := []FunctionData{
		{
			Name:                 testFunctionName,
			CyclomaticComplexity: CyclomaticThresholdHigh,
			CognitiveComplexity:  0,
			NestingDepth:         0,
		},
	}
	metric := NewHighRiskFunctionMetric()
	input := makeTestReportData(functions)

	result := metric.Compute(input)

	require.Len(t, result, 1)
	assert.Equal(t, testFunctionName, result[0].Name)
	assert.Contains(t, result[0].Issues, "High cyclomatic complexity")
	assert.Len(t, result[0].Issues, 1)
}

func TestHighRiskFunctionMetric_MultipleIssues(t *testing.T) {
	t.Parallel()

	functions := []FunctionData{
		{
			Name:                 testFunctionName,
			CyclomaticComplexity: CyclomaticThresholdHigh,
			CognitiveComplexity:  CognitiveThresholdHigh,
			NestingDepth:         NestingThresholdHigh,
		},
	}
	metric := NewHighRiskFunctionMetric()
	input := makeTestReportData(functions)

	result := metric.Compute(input)

	require.Len(t, result, 1)
	assert.Len(t, result[0].Issues, 3)
	assert.Contains(t, result[0].Issues, "High cyclomatic complexity")
	assert.Contains(t, result[0].Issues, "High cognitive complexity")
	assert.Contains(t, result[0].Issues, "Deep nesting")
	assert.Equal(t, "CRITICAL", result[0].RiskLevel)
}

func TestHighRiskFunctionMetric_SortedByRisk(t *testing.T) {
	t.Parallel()

	functions := []FunctionData{
		{
			Name:                 "medium_risk",
			CyclomaticComplexity: CyclomaticThresholdHigh,
			CognitiveComplexity:  0,
			NestingDepth:         0,
		},
		{
			Name:                 "critical_risk",
			CyclomaticComplexity: CyclomaticThresholdHigh,
			CognitiveComplexity:  CognitiveThresholdHigh,
			NestingDepth:         NestingThresholdHigh,
		},
	}
	metric := NewHighRiskFunctionMetric()
	input := makeTestReportData(functions)

	result := metric.Compute(input)

	require.Len(t, result, 2)
	// Critical should come first.
	assert.Equal(t, "critical_risk", result[0].Name)
	assert.Equal(t, "CRITICAL", result[0].RiskLevel)
	assert.Equal(t, "medium_risk", result[1].Name)
}

// --- ComplexityAggregateMetric Tests ---.

func TestComplexityAggregateMetric_Metadata(t *testing.T) {
	t.Parallel()

	metric := NewAggregateMetric()

	assert.Equal(t, "complexity_aggregate", metric.Name())
	assert.Equal(t, "Complexity Summary", metric.DisplayName())
	assert.Equal(t, "aggregate", metric.Type())
	assert.NotEmpty(t, metric.Description())
}

func TestComplexityAggregateMetric_Compute(t *testing.T) {
	t.Parallel()

	input := &ReportData{
		TotalFunctions:      testTotalFunctions,
		AverageComplexity:   testAverageComplexity,
		MaxComplexity:       testMaxComplexity,
		TotalComplexity:     testTotalComplexity,
		CognitiveComplexity: testCognitiveComplexity,
		NestingDepth:        testNestingDepth,
		DecisionPoints:      testDecisionPoints,
		Message:             testMessage,
	}
	metric := NewAggregateMetric()

	result := metric.Compute(input)

	assert.Equal(t, testTotalFunctions, result.TotalFunctions)
	assert.InDelta(t, testAverageComplexity, result.AverageComplexity, 0.001)
	assert.Equal(t, testMaxComplexity, result.MaxComplexity)
	assert.Equal(t, testTotalComplexity, result.TotalComplexity)
	assert.Equal(t, testCognitiveComplexity, result.CognitiveComplexity)
	assert.Equal(t, testNestingDepth, result.NestingDepth)
	assert.Equal(t, testDecisionPoints, result.DecisionPoints)
	assert.Equal(t, testMessage, result.Message)
}

func TestComplexityAggregateMetric_HealthScore(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name              string
		averageComplexity float64
		expectedScore     float64
	}{
		{
			name:              "Perfect health - complexity <= 1",
			averageComplexity: 1.0,
			expectedScore:     100.0,
		},
		{
			name:              "Very low complexity",
			averageComplexity: 0.5,
			expectedScore:     100.0,
		},
		{
			name:              "Good health - complexity 2.0",
			averageComplexity: 2.0,
			expectedScore:     90.0,
		},
		{
			name:              "Good health - complexity 3.0",
			averageComplexity: 3.0,
			expectedScore:     80.0,
		},
		{
			name:              "Moderate health - complexity 5.0",
			averageComplexity: 5.0,
			expectedScore:     65.0,
		},
		{
			name:              "Moderate health - complexity 7.0",
			averageComplexity: 7.0,
			expectedScore:     50.0,
		},
		{
			name:              "Poor health - complexity 10.0",
			averageComplexity: 10.0,
			expectedScore:     35.0,
		},
		{
			name:              "Very poor health - complexity 17.0",
			averageComplexity: 17.0,
			expectedScore:     0.0,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			input := &ReportData{AverageComplexity: tt.averageComplexity}
			metric := NewAggregateMetric()

			result := metric.Compute(input)

			assert.InDelta(t, tt.expectedScore, result.HealthScore, 0.001)
		})
	}
}

// --- riskPriority Tests ---.

func TestRiskPriority(t *testing.T) {
	t.Parallel()

	tests := []struct {
		level    string
		priority int
	}{
		{"CRITICAL", 0},
		{"HIGH", 1},
		{"MEDIUM", 2},
		{"LOW", 3},
		{"UNKNOWN", 3}, // default case.
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

func TestComputeAllMetrics_Empty(t *testing.T) {
	t.Parallel()

	report := analyze.Report{}

	result, err := ComputeAllMetrics(report)

	require.NoError(t, err)
	require.NotNil(t, result)
	assert.Empty(t, result.FunctionComplexity)
	assert.Empty(t, result.HighRiskFunctions)
	assert.Equal(t, 0, result.Distribution.Simple)
	assert.Equal(t, 0, result.Distribution.Moderate)
	assert.Equal(t, 0, result.Distribution.Complex)
}

func TestComputeAllMetrics_FullReport(t *testing.T) {
	t.Parallel()

	report := analyze.Report{
		"total_functions":      3,
		"average_complexity":   testAverageComplexity,
		"max_complexity":       testMaxComplexity,
		"total_complexity":     testTotalComplexity,
		"cognitive_complexity": testCognitiveComplexity,
		"nesting_depth":        testNestingDepth,
		"decision_points":      testDecisionPoints,
		"message":              testMessage,
		"functions": []map[string]any{
			{
				"name":                  "simple_func",
				"cyclomatic_complexity": 3,
				"cognitive_complexity":  2,
				"nesting_depth":         1,
				"lines_of_code":         10,
			},
			{
				"name":                  "moderate_func",
				"cyclomatic_complexity": 8,
				"cognitive_complexity":  10,
				"nesting_depth":         3,
				"lines_of_code":         50,
			},
			{
				"name":                  "complex_func",
				"cyclomatic_complexity": CyclomaticThresholdHigh + 5,
				"cognitive_complexity":  CognitiveThresholdHigh + 5,
				"nesting_depth":         NestingThresholdHigh + 1,
				"lines_of_code":         200,
			},
		},
	}

	result, err := ComputeAllMetrics(report)

	require.NoError(t, err)
	require.NotNil(t, result)

	// FunctionComplexity - sorted by cyclomatic desc.
	require.Len(t, result.FunctionComplexity, 3)
	assert.Equal(t, "complex_func", result.FunctionComplexity[0].Name)
	assert.Equal(t, "moderate_func", result.FunctionComplexity[1].Name)
	assert.Equal(t, "simple_func", result.FunctionComplexity[2].Name)

	// Distribution.
	assert.Equal(t, 1, result.Distribution.Simple)   // simple_func (3).
	assert.Equal(t, 1, result.Distribution.Moderate) // moderate_func (8).
	assert.Equal(t, 1, result.Distribution.Complex)  // complex_func (15).

	// HighRiskFunctions - only complex_func exceeds thresholds.
	require.Len(t, result.HighRiskFunctions, 1)
	assert.Equal(t, "complex_func", result.HighRiskFunctions[0].Name)
	assert.Equal(t, "CRITICAL", result.HighRiskFunctions[0].RiskLevel)

	// Aggregate.
	assert.Equal(t, 3, result.Aggregate.TotalFunctions)
	assert.InDelta(t, testAverageComplexity, result.Aggregate.AverageComplexity, 0.001)
	assert.Equal(t, testMessage, result.Aggregate.Message)
}

// --- MetricsOutput Interface Tests ---.

func TestComputedMetrics_AnalyzerName(t *testing.T) {
	t.Parallel()

	metrics := &ComputedMetrics{}

	name := metrics.AnalyzerName()

	assert.Equal(t, "complexity", name)
}

func TestComputedMetrics_ToJSON(t *testing.T) {
	t.Parallel()

	metrics := &ComputedMetrics{
		Aggregate: AggregateData{
			TotalFunctions:    testTotalFunctions,
			AverageComplexity: testAverageComplexity,
		},
	}

	result := metrics.ToJSON()

	assert.Equal(t, metrics, result)
}

func TestComputedMetrics_ToYAML(t *testing.T) {
	t.Parallel()

	metrics := &ComputedMetrics{
		Aggregate: AggregateData{
			TotalFunctions:    testTotalFunctions,
			AverageComplexity: testAverageComplexity,
		},
	}

	result := metrics.ToYAML()

	assert.Equal(t, metrics, result)
}
