package comments

import (
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
)

// Test constants to avoid magic strings/numbers.
const (
	testFunctionName1  = "TestFunc1"
	testFunctionName2  = "TestFunc2"
	testFunctionName3  = "TestFunc3"
	testMessage        = "Test comments message"
	testTargetName     = "TestTarget"
	testRecommendation = "Add description"

	testTotalComments       = 10
	testGoodComments        = 7
	testBadComments         = 3
	testOverallScore        = 0.75
	testTotalFunctions      = 5
	testDocumentedFunctions = 3
	testGoodCommentsRatio   = 0.7
	testDocCoverage         = 0.6
	testCommentScore        = 0.8

	floatDelta = 0.01
)

// --- ParseReportData Tests ---

func TestParseReportData_Empty(t *testing.T) {
	report := analyze.Report{}

	result, err := ParseReportData(report)

	require.NoError(t, err)
	assert.Equal(t, 0, result.TotalComments)
	assert.Empty(t, result.Comments)
	assert.Empty(t, result.Functions)
	assert.Empty(t, result.Message)
}

func TestParseReportData_AllFields(t *testing.T) {
	report := analyze.Report{
		"total_comments":         testTotalComments,
		"good_comments":          testGoodComments,
		"bad_comments":           testBadComments,
		"overall_score":          testOverallScore,
		"total_functions":        testTotalFunctions,
		"documented_functions":   testDocumentedFunctions,
		"good_comments_ratio":    testGoodCommentsRatio,
		"documentation_coverage": testDocCoverage,
		"message":                testMessage,
	}

	result, err := ParseReportData(report)

	require.NoError(t, err)
	assert.Equal(t, testTotalComments, result.TotalComments)
	assert.Equal(t, testGoodComments, result.GoodComments)
	assert.Equal(t, testBadComments, result.BadComments)
	assert.InDelta(t, testOverallScore, result.OverallScore, floatDelta)
	assert.Equal(t, testTotalFunctions, result.TotalFunctions)
	assert.Equal(t, testDocumentedFunctions, result.DocumentedFunctions)
	assert.InDelta(t, testGoodCommentsRatio, result.GoodCommentsRatio, floatDelta)
	assert.InDelta(t, testDocCoverage, result.DocumentationCoverage, floatDelta)
	assert.Equal(t, testMessage, result.Message)
}

func TestParseReportData_WithComments(t *testing.T) {
	report := analyze.Report{
		"comments": []map[string]any{
			{
				"line":           10,
				"quality":        "good",
				"type":           "docstring",
				"target_type":    "function",
				"target_name":    testTargetName,
				"position":       "above",
				"recommendation": testRecommendation,
				"score":          testCommentScore,
			},
			{
				"line":    20,
				"quality": "bad",
			},
		},
	}

	result, err := ParseReportData(report)

	require.NoError(t, err)
	require.Len(t, result.Comments, 2)

	// First comment - all fields
	c1 := result.Comments[0]
	assert.Equal(t, 10, c1.LineNumber)
	assert.Equal(t, "good", c1.Quality)
	assert.Equal(t, "docstring", c1.Type)
	assert.Equal(t, "function", c1.TargetType)
	assert.Equal(t, testTargetName, c1.TargetName)
	assert.Equal(t, "above", c1.Position)
	assert.Equal(t, testRecommendation, c1.Recommendation)
	assert.InDelta(t, testCommentScore, c1.Score, floatDelta)

	// Second comment - minimal fields
	c2 := result.Comments[1]
	assert.Equal(t, 20, c2.LineNumber)
	assert.Equal(t, "bad", c2.Quality)
}

func TestParseReportData_WithFunctions(t *testing.T) {
	report := analyze.Report{
		"functions": []map[string]any{
			{
				"name":          testFunctionName1,
				"has_comment":   true,
				"needs_comment": false,
				"comment_score": testCommentScore,
				"comment_type":  "docstring",
			},
			{
				"name":          testFunctionName2,
				"has_comment":   false,
				"needs_comment": true,
			},
		},
	}

	result, err := ParseReportData(report)

	require.NoError(t, err)
	require.Len(t, result.Functions, 2)

	// First function - all fields
	fn1 := result.Functions[0]
	assert.Equal(t, testFunctionName1, fn1.Name)
	assert.True(t, fn1.HasComment)
	assert.False(t, fn1.NeedsComment)
	assert.InDelta(t, testCommentScore, fn1.CommentScore, floatDelta)
	assert.Equal(t, "docstring", fn1.CommentType)

	// Second function - minimal fields
	fn2 := result.Functions[1]
	assert.Equal(t, testFunctionName2, fn2.Name)
	assert.False(t, fn2.HasComment)
	assert.True(t, fn2.NeedsComment)
}

// --- CommentQualityMetric Tests ---

func TestNewCommentQualityMetric_Metadata(t *testing.T) {
	m := NewCommentQualityMetric()

	assert.Equal(t, "comment_quality", m.Name())
	assert.Equal(t, "Comment Quality", m.DisplayName())
	assert.Contains(t, m.Description(), "Per-comment quality")
	assert.Equal(t, "list", m.Type())
}

func TestCommentQualityMetric_Empty(t *testing.T) {
	m := NewCommentQualityMetric()
	input := &ReportData{}

	result := m.Compute(input)

	assert.Empty(t, result)
}

func TestCommentQualityMetric_SingleComment(t *testing.T) {
	m := NewCommentQualityMetric()
	input := &ReportData{
		Comments: []CommentData{
			{
				LineNumber:     10,
				Quality:        "good",
				Type:           "docstring",
				TargetName:     testTargetName,
				Score:          testCommentScore,
				Recommendation: testRecommendation,
			},
		},
	}

	result := m.Compute(input)

	require.Len(t, result, 1)
	assert.Equal(t, 10, result[0].LineNumber)
	assert.Equal(t, "good", result[0].Quality)
	assert.Equal(t, "docstring", result[0].Type)
	assert.Equal(t, testTargetName, result[0].TargetName)
	assert.InDelta(t, testCommentScore, result[0].Score, floatDelta)
	assert.Equal(t, testRecommendation, result[0].Recommendation)
}

func TestCommentQualityMetric_SortedByLineNumber(t *testing.T) {
	m := NewCommentQualityMetric()
	input := &ReportData{
		Comments: []CommentData{
			{LineNumber: 50, Quality: "good"},
			{LineNumber: 10, Quality: "bad"},
			{LineNumber: 30, Quality: "good"},
		},
	}

	result := m.Compute(input)

	require.Len(t, result, 3)
	assert.Equal(t, 10, result[0].LineNumber)
	assert.Equal(t, 30, result[1].LineNumber)
	assert.Equal(t, 50, result[2].LineNumber)
}

// --- FunctionDocumentationMetric Tests ---

func TestNewFunctionDocumentationMetric_Metadata(t *testing.T) {
	m := NewFunctionDocumentationMetric()

	assert.Equal(t, "function_documentation", m.Name())
	assert.Equal(t, "Function Documentation", m.DisplayName())
	assert.Contains(t, m.Description(), "Per-function documentation")
	assert.Equal(t, "list", m.Type())
}

func TestFunctionDocumentationMetric_Empty(t *testing.T) {
	m := NewFunctionDocumentationMetric()
	input := &ReportData{}

	result := m.Compute(input)

	assert.Empty(t, result)
}

func TestFunctionDocumentationMetric_DocumentationStatuses(t *testing.T) {
	tests := []struct {
		name           string
		hasComment     bool
		commentScore   float64
		expectedStatus string
	}{
		{"undocumented", false, 0.0, "Undocumented"},
		{"well_documented", true, 0.8, "Well Documented"},
		{"well_documented_boundary", true, 0.6, "Well Documented"},
		{"partially_documented", true, 0.5, "Partially Documented"},
		{"partially_documented_boundary", true, 0.3, "Partially Documented"},
		{"poorly_documented", true, 0.2, "Poorly Documented"},
		{"poorly_documented_zero", true, 0.0, "Poorly Documented"},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			m := NewFunctionDocumentationMetric()
			input := &ReportData{
				Functions: []FunctionCommentData{
					{
						Name:         testFunctionName1,
						HasComment:   tt.hasComment,
						CommentScore: tt.commentScore,
					},
				},
			}

			result := m.Compute(input)

			require.Len(t, result, 1)
			assert.Equal(t, testFunctionName1, result[0].Name)
			assert.Equal(t, tt.hasComment, result[0].IsDocumented)
			assert.InDelta(t, tt.commentScore, result[0].DocumentationScore, floatDelta)
			assert.Equal(t, tt.expectedStatus, result[0].Status)
		})
	}
}

func TestFunctionDocumentationMetric_SortedByScoreAscending(t *testing.T) {
	m := NewFunctionDocumentationMetric()
	input := &ReportData{
		Functions: []FunctionCommentData{
			{Name: testFunctionName1, HasComment: true, CommentScore: 0.8},
			{Name: testFunctionName2, HasComment: true, CommentScore: 0.2},
			{Name: testFunctionName3, HasComment: true, CommentScore: 0.5},
		},
	}

	result := m.Compute(input)

	require.Len(t, result, 3)
	// Sorted by score ascending (worst first)
	assert.Equal(t, testFunctionName2, result[0].Name)
	assert.Equal(t, testFunctionName3, result[1].Name)
	assert.Equal(t, testFunctionName1, result[2].Name)
}

// --- UndocumentedFunctionMetric Tests ---

func TestNewUndocumentedFunctionMetric_Metadata(t *testing.T) {
	m := NewUndocumentedFunctionMetric()

	assert.Equal(t, "undocumented_functions", m.Name())
	assert.Equal(t, "Undocumented Functions", m.DisplayName())
	assert.Contains(t, m.Description(), "lack documentation")
	assert.Equal(t, "risk", m.Type())
}

func TestUndocumentedFunctionMetric_Empty(t *testing.T) {
	m := NewUndocumentedFunctionMetric()
	input := &ReportData{}

	result := m.Compute(input)

	assert.Empty(t, result)
}

func TestUndocumentedFunctionMetric_AllDocumented(t *testing.T) {
	m := NewUndocumentedFunctionMetric()
	input := &ReportData{
		Functions: []FunctionCommentData{
			{Name: testFunctionName1, HasComment: true},
			{Name: testFunctionName2, HasComment: true},
		},
	}

	result := m.Compute(input)

	assert.Empty(t, result)
}

func TestUndocumentedFunctionMetric_RiskLevels(t *testing.T) {
	tests := []struct {
		name         string
		needsComment bool
		expected     string
	}{
		{"high_risk_needs_comment", true, "HIGH"},
		{"medium_risk_optional", false, "MEDIUM"},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			m := NewUndocumentedFunctionMetric()
			input := &ReportData{
				Functions: []FunctionCommentData{
					{
						Name:         testFunctionName1,
						HasComment:   false,
						NeedsComment: tt.needsComment,
					},
				},
			}

			result := m.Compute(input)

			require.Len(t, result, 1)
			assert.Equal(t, testFunctionName1, result[0].Name)
			assert.Equal(t, tt.needsComment, result[0].NeedsComment)
			assert.Equal(t, tt.expected, result[0].RiskLevel)
		})
	}
}

func TestUndocumentedFunctionMetric_SortedByRisk(t *testing.T) {
	m := NewUndocumentedFunctionMetric()
	input := &ReportData{
		Functions: []FunctionCommentData{
			{Name: testFunctionName1, HasComment: false, NeedsComment: false}, // MEDIUM
			{Name: testFunctionName2, HasComment: false, NeedsComment: true},  // HIGH
			{Name: testFunctionName3, HasComment: false, NeedsComment: false}, // MEDIUM
		},
	}

	result := m.Compute(input)

	require.Len(t, result, 3)
	// HIGH risk first
	assert.Equal(t, testFunctionName2, result[0].Name)
	assert.Equal(t, "HIGH", result[0].RiskLevel)
}

// --- riskPriority Tests ---

func TestRiskPriority(t *testing.T) {
	tests := []struct {
		level    string
		expected int
	}{
		{"HIGH", 0},
		{"MEDIUM", 1},
		{"LOW", 2},
		{"UNKNOWN", 2},
		{"", 2},
	}

	for _, tt := range tests {
		t.Run(tt.level, func(t *testing.T) {
			result := riskPriority(tt.level)
			assert.Equal(t, tt.expected, result)
		})
	}
}

// --- CommentsAggregateMetric Tests ---

func TestNewAggregateMetric_Metadata(t *testing.T) {
	m := NewAggregateMetric()

	assert.Equal(t, "comments_aggregate", m.Name())
	assert.Equal(t, "Comments Summary", m.DisplayName())
	assert.Contains(t, m.Description(), "Aggregate comment statistics")
	assert.Equal(t, "aggregate", m.Type())
}

func TestCommentsAggregateMetric_Empty(t *testing.T) {
	m := NewAggregateMetric()
	input := &ReportData{}

	result := m.Compute(input)

	assert.Equal(t, 0, result.TotalComments)
	assert.InDelta(t, 0.0, result.OverallScore, floatDelta)
	assert.InDelta(t, 0.0, result.HealthScore, floatDelta)
}

func TestCommentsAggregateMetric_AllFields(t *testing.T) {
	m := NewAggregateMetric()
	input := &ReportData{
		TotalComments:         testTotalComments,
		GoodComments:          testGoodComments,
		BadComments:           testBadComments,
		OverallScore:          testOverallScore,
		TotalFunctions:        testTotalFunctions,
		DocumentedFunctions:   testDocumentedFunctions,
		GoodCommentsRatio:     testGoodCommentsRatio,
		DocumentationCoverage: testDocCoverage,
		Message:               testMessage,
	}

	result := m.Compute(input)

	assert.Equal(t, testTotalComments, result.TotalComments)
	assert.Equal(t, testGoodComments, result.GoodComments)
	assert.Equal(t, testBadComments, result.BadComments)
	assert.InDelta(t, testOverallScore, result.OverallScore, floatDelta)
	assert.Equal(t, testTotalFunctions, result.TotalFunctions)
	assert.Equal(t, testDocumentedFunctions, result.DocumentedFunctions)
	assert.InDelta(t, testGoodCommentsRatio, result.GoodCommentsRatio, floatDelta)
	assert.InDelta(t, testDocCoverage, result.DocumentationCoverage, floatDelta)
	assert.Equal(t, testMessage, result.Message)
	// Health score = OverallScore * 100
	assert.InDelta(t, testOverallScore*100, result.HealthScore, floatDelta)
}

func TestCommentsAggregateMetric_HealthScoreCalculation(t *testing.T) {
	tests := []struct {
		name         string
		overallScore float64
		expected     float64
	}{
		{"perfect", 1.0, 100.0},
		{"good", 0.75, 75.0},
		{"medium", 0.5, 50.0},
		{"poor", 0.25, 25.0},
		{"zero", 0.0, 0.0},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			m := NewAggregateMetric()
			input := &ReportData{
				OverallScore: tt.overallScore,
			}

			result := m.Compute(input)

			assert.InDelta(t, tt.expected, result.HealthScore, floatDelta)
		})
	}
}

// --- ComputeAllMetrics Tests ---

func TestComputeAllMetrics_Empty(t *testing.T) {
	report := analyze.Report{}

	result, err := ComputeAllMetrics(report)

	require.NoError(t, err)
	assert.Empty(t, result.CommentQuality)
	assert.Empty(t, result.FunctionDocumentation)
	assert.Empty(t, result.UndocumentedFunctions)
	assert.Equal(t, 0, result.Aggregate.TotalComments)
}

func TestComputeAllMetrics_Full(t *testing.T) {
	report := analyze.Report{
		"total_comments":         testTotalComments,
		"good_comments":          testGoodComments,
		"bad_comments":           testBadComments,
		"overall_score":          testOverallScore,
		"total_functions":        testTotalFunctions,
		"documented_functions":   testDocumentedFunctions,
		"good_comments_ratio":    testGoodCommentsRatio,
		"documentation_coverage": testDocCoverage,
		"message":                testMessage,
		"comments": []map[string]any{
			{"line": 10, "quality": "good", "target_name": testTargetName, "score": 0.9},
			{"line": 20, "quality": "bad", "target_name": "BadTarget", "score": 0.2},
		},
		"functions": []map[string]any{
			{"name": testFunctionName1, "has_comment": true, "comment_score": 0.8},
			{"name": testFunctionName2, "has_comment": false, "needs_comment": true},
			{"name": testFunctionName3, "has_comment": false, "needs_comment": false},
		},
	}

	result, err := ComputeAllMetrics(report)

	require.NoError(t, err)

	// CommentQuality - sorted by line number
	require.Len(t, result.CommentQuality, 2)
	assert.Equal(t, 10, result.CommentQuality[0].LineNumber)
	assert.Equal(t, 20, result.CommentQuality[1].LineNumber)

	// FunctionDocumentation - sorted by score ascending
	require.Len(t, result.FunctionDocumentation, 3)
	// Undocumented functions have score 0, so they come first
	assert.False(t, result.FunctionDocumentation[0].IsDocumented)

	// UndocumentedFunctions - only functions without comments
	require.Len(t, result.UndocumentedFunctions, 2)
	// HIGH risk first (needs_comment = true)
	assert.Equal(t, testFunctionName2, result.UndocumentedFunctions[0].Name)
	assert.Equal(t, "HIGH", result.UndocumentedFunctions[0].RiskLevel)

	// Aggregate
	assert.Equal(t, testTotalComments, result.Aggregate.TotalComments)
	assert.Equal(t, testMessage, result.Aggregate.Message)
	assert.InDelta(t, testOverallScore*100, result.Aggregate.HealthScore, floatDelta)
}

// --- MetricsOutput Interface Tests ---

func TestComputedMetrics_AnalyzerName(t *testing.T) {
	m := &ComputedMetrics{}

	assert.Equal(t, "comments", m.AnalyzerName())
}

func TestComputedMetrics_ToJSON(t *testing.T) {
	m := &ComputedMetrics{
		CommentQuality: []CommentQualityData{
			{LineNumber: 10, Quality: "good"},
		},
		Aggregate: AggregateData{TotalComments: 1},
	}

	result := m.ToJSON()

	assert.Equal(t, m, result)
}

func TestComputedMetrics_ToYAML(t *testing.T) {
	m := &ComputedMetrics{
		CommentQuality: []CommentQualityData{
			{LineNumber: 10, Quality: "good"},
		},
		Aggregate: AggregateData{TotalComments: 1},
	}

	result := m.ToYAML()

	assert.Equal(t, m, result)
}
