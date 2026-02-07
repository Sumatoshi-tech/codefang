package comments

import (
	"sort"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/metrics"
)

// ReportData is the parsed input data for comments metrics computation.
type ReportData struct {
	TotalComments         int
	GoodComments          int
	BadComments           int
	OverallScore          float64
	TotalFunctions        int
	DocumentedFunctions   int
	GoodCommentsRatio     float64
	DocumentationCoverage float64
	Comments              []CommentData
	Functions             []FunctionCommentData
	Message               string
}

// CommentData holds data for a single comment.
type CommentData struct {
	LineNumber     int
	Quality        string
	Type           string
	TargetType     string
	TargetName     string
	Position       string
	Recommendation string
	Score          float64
}

// FunctionCommentData holds comment data for a function.
type FunctionCommentData struct {
	Name         string
	HasComment   bool
	NeedsComment bool
	CommentScore float64
	CommentType  string
}

// ParseReportData extracts ReportData from an analyzer report.
func ParseReportData(report analyze.Report) (*ReportData, error) {
	data := &ReportData{}
	parseReportScalars(data, report)
	data.Comments = parseReportComments(report)
	data.Functions = parseReportFunctions(report)

	return data, nil
}

func parseReportScalars(data *ReportData, report analyze.Report) {
	if v, ok := report["total_comments"].(int); ok {
		data.TotalComments = v
	}

	if v, ok := report["good_comments"].(int); ok {
		data.GoodComments = v
	}

	if v, ok := report["bad_comments"].(int); ok {
		data.BadComments = v
	}

	if v, ok := report["overall_score"].(float64); ok {
		data.OverallScore = v
	}

	if v, ok := report["total_functions"].(int); ok {
		data.TotalFunctions = v
	}

	if v, ok := report["documented_functions"].(int); ok {
		data.DocumentedFunctions = v
	}

	if v, ok := report["good_comments_ratio"].(float64); ok {
		data.GoodCommentsRatio = v
	}

	if v, ok := report["documentation_coverage"].(float64); ok {
		data.DocumentationCoverage = v
	}

	if v, ok := report["message"].(string); ok {
		data.Message = v
	}
}

func parseReportComments(report analyze.Report) []CommentData {
	comments, ok := report["comments"].([]map[string]any)
	if !ok {
		return nil
	}

	result := make([]CommentData, 0, len(comments))

	for _, comment := range comments {
		result = append(result, parseComment(comment))
	}

	return result
}

func parseComment(comment map[string]any) CommentData {
	cd := CommentData{}

	if v, ok := comment["line"].(int); ok {
		cd.LineNumber = v
	}

	if v, ok := comment["quality"].(string); ok {
		cd.Quality = v
	}

	if v, ok := comment["type"].(string); ok {
		cd.Type = v
	}

	if v, ok := comment["target_type"].(string); ok {
		cd.TargetType = v
	}

	if v, ok := comment["target_name"].(string); ok {
		cd.TargetName = v
	}

	if v, ok := comment["position"].(string); ok {
		cd.Position = v
	}

	if v, ok := comment["recommendation"].(string); ok {
		cd.Recommendation = v
	}

	if v, ok := comment["score"].(float64); ok {
		cd.Score = v
	}

	return cd
}

func parseReportFunctions(report analyze.Report) []FunctionCommentData {
	functions, ok := report["functions"].([]map[string]any)
	if !ok {
		return nil
	}

	result := make([]FunctionCommentData, 0, len(functions))

	for _, fn := range functions {
		result = append(result, parseFunctionComment(fn))
	}

	return result
}

func parseFunctionComment(fn map[string]any) FunctionCommentData {
	fd := FunctionCommentData{}

	if v, ok := fn["name"].(string); ok {
		fd.Name = v
	}

	if v, ok := fn["has_comment"].(bool); ok {
		fd.HasComment = v
	}

	if v, ok := fn["needs_comment"].(bool); ok {
		fd.NeedsComment = v
	}

	if v, ok := fn["comment_score"].(float64); ok {
		fd.CommentScore = v
	}

	if v, ok := fn["comment_type"].(string); ok {
		fd.CommentType = v
	}

	return fd
}

// --- Output Data Types ---

// CommentQualityData contains quality assessment for a comment.
type CommentQualityData struct {
	LineNumber     int     `json:"line_number"              yaml:"line_number"`
	Quality        string  `json:"quality"                  yaml:"quality"`
	Type           string  `json:"type"                     yaml:"type"`
	TargetName     string  `json:"target_name"              yaml:"target_name"`
	Score          float64 `json:"score"                    yaml:"score"`
	Recommendation string  `json:"recommendation,omitempty" yaml:"recommendation,omitempty"`
}

// FunctionDocumentationData contains documentation status for a function.
type FunctionDocumentationData struct {
	Name               string  `json:"name"                yaml:"name"`
	IsDocumented       bool    `json:"is_documented"       yaml:"is_documented"`
	DocumentationScore float64 `json:"documentation_score" yaml:"documentation_score"`
	Status             string  `json:"status"              yaml:"status"`
}

// UndocumentedFunctionData identifies functions lacking documentation.
type UndocumentedFunctionData struct {
	Name         string `json:"name"          yaml:"name"`
	NeedsComment bool   `json:"needs_comment" yaml:"needs_comment"`
	RiskLevel    string `json:"risk_level"    yaml:"risk_level"`
}

// AggregateData contains summary statistics.
type AggregateData struct {
	TotalComments         int     `json:"total_comments"         yaml:"total_comments"`
	GoodComments          int     `json:"good_comments"          yaml:"good_comments"`
	BadComments           int     `json:"bad_comments"           yaml:"bad_comments"`
	OverallScore          float64 `json:"overall_score"          yaml:"overall_score"`
	TotalFunctions        int     `json:"total_functions"        yaml:"total_functions"`
	DocumentedFunctions   int     `json:"documented_functions"   yaml:"documented_functions"`
	GoodCommentsRatio     float64 `json:"good_comments_ratio"    yaml:"good_comments_ratio"`
	DocumentationCoverage float64 `json:"documentation_coverage" yaml:"documentation_coverage"`
	HealthScore           float64 `json:"health_score"           yaml:"health_score"`
	Message               string  `json:"message"                yaml:"message"`
}

// CommentQualityMetric computes per-comment quality data.
type CommentQualityMetric struct {
	metrics.MetricMeta
}

// NewCommentQualityMetric creates the comment quality metric.
func NewCommentQualityMetric() *CommentQualityMetric {
	return &CommentQualityMetric{
		MetricMeta: metrics.MetricMeta{
			MetricName:        "comment_quality",
			MetricDisplayName: "Comment Quality",
			MetricDescription: "Per-comment quality assessment based on placement, content, and relevance. " +
				"Good comments are well-placed and informative; bad comments may be misplaced or redundant.",
			MetricType: "list",
		},
	}
}

// Compute calculates comment quality data.
func (m *CommentQualityMetric) Compute(input *ReportData) []CommentQualityData {
	result := make([]CommentQualityData, 0, len(input.Comments))

	for _, comment := range input.Comments {
		result = append(result, CommentQualityData{
			LineNumber:     comment.LineNumber,
			Quality:        comment.Quality,
			Type:           comment.Type,
			TargetName:     comment.TargetName,
			Score:          comment.Score,
			Recommendation: comment.Recommendation,
		})
	}

	// Sort by line number
	sort.Slice(result, func(i, j int) bool {
		return result[i].LineNumber < result[j].LineNumber
	})

	return result
}

// FunctionDocumentationMetric computes per-function documentation status.
type FunctionDocumentationMetric struct {
	metrics.MetricMeta
}

// NewFunctionDocumentationMetric creates the function documentation metric.
func NewFunctionDocumentationMetric() *FunctionDocumentationMetric {
	return &FunctionDocumentationMetric{
		MetricMeta: metrics.MetricMeta{
			MetricName:        "function_documentation",
			MetricDisplayName: "Function Documentation",
			MetricDescription: "Per-function documentation status showing which functions have comments " +
				"and their documentation quality scores.",
			MetricType: "list",
		},
	}
}

// Documentation score thresholds and constants.
const (
	DocScoreThresholdGood = 0.6
	DocScoreThresholdFair = 0.3

	// Risk priority values for sorting.
	riskPriorityHigh    = 0
	riskPriorityMedium  = 1
	riskPriorityDefault = 2

	// Health score multiplier (converts 0-1 score to 0-100).
	healthScoreMultiplier = 100
)

// Compute calculates function documentation data.
func (m *FunctionDocumentationMetric) Compute(input *ReportData) []FunctionDocumentationData {
	result := make([]FunctionDocumentationData, 0, len(input.Functions))

	for _, fn := range input.Functions {
		var status string

		switch {
		case !fn.HasComment:
			status = "Undocumented"
		case fn.CommentScore >= DocScoreThresholdGood:
			status = "Well Documented"
		case fn.CommentScore >= DocScoreThresholdFair:
			status = "Partially Documented"
		default:
			status = "Poorly Documented"
		}

		result = append(result, FunctionDocumentationData{
			Name:               fn.Name,
			IsDocumented:       fn.HasComment,
			DocumentationScore: fn.CommentScore,
			Status:             status,
		})
	}

	// Sort by documentation score ascending (worst first)
	sort.Slice(result, func(i, j int) bool {
		return result[i].DocumentationScore < result[j].DocumentationScore
	})

	return result
}

// UndocumentedFunctionMetric identifies functions lacking documentation.
type UndocumentedFunctionMetric struct {
	metrics.MetricMeta
}

// NewUndocumentedFunctionMetric creates the undocumented function metric.
func NewUndocumentedFunctionMetric() *UndocumentedFunctionMetric {
	return &UndocumentedFunctionMetric{
		MetricMeta: metrics.MetricMeta{
			MetricName:        "undocumented_functions",
			MetricDisplayName: "Undocumented Functions",
			MetricDescription: "Functions that lack documentation comments. These may need documentation " +
				"especially if they are public APIs or complex logic.",
			MetricType: "risk",
		},
	}
}

// Compute identifies undocumented functions.
func (m *UndocumentedFunctionMetric) Compute(input *ReportData) []UndocumentedFunctionData {
	result := make([]UndocumentedFunctionData, 0)

	for _, fn := range input.Functions {
		if fn.HasComment {
			continue
		}

		var riskLevel string
		if fn.NeedsComment {
			riskLevel = "HIGH"
		} else {
			riskLevel = "MEDIUM"
		}

		result = append(result, UndocumentedFunctionData{
			Name:         fn.Name,
			NeedsComment: fn.NeedsComment,
			RiskLevel:    riskLevel,
		})
	}

	// Sort by risk level
	sort.Slice(result, func(i, j int) bool {
		return riskPriority(result[i].RiskLevel) < riskPriority(result[j].RiskLevel)
	})

	return result
}

func riskPriority(level string) int {
	switch level {
	case "HIGH":
		return riskPriorityHigh
	case "MEDIUM":
		return riskPriorityMedium
	default:
		return riskPriorityDefault
	}
}

// AggregateMetric computes summary statistics.
type AggregateMetric struct {
	metrics.MetricMeta
}

// NewAggregateMetric creates the aggregate metric.
func NewAggregateMetric() *AggregateMetric {
	return &AggregateMetric{
		MetricMeta: metrics.MetricMeta{
			MetricName:        "comments_aggregate",
			MetricDisplayName: "Comments Summary",
			MetricDescription: "Aggregate comment statistics including total count, quality ratio, " +
				"and documentation coverage. Health score reflects overall comment quality (0-100).",
			MetricType: "aggregate",
		},
	}
}

// Compute calculates aggregate statistics.
func (m *AggregateMetric) Compute(input *ReportData) AggregateData {
	agg := AggregateData{
		TotalComments:         input.TotalComments,
		GoodComments:          input.GoodComments,
		BadComments:           input.BadComments,
		OverallScore:          input.OverallScore,
		TotalFunctions:        input.TotalFunctions,
		DocumentedFunctions:   input.DocumentedFunctions,
		GoodCommentsRatio:     input.GoodCommentsRatio,
		DocumentationCoverage: input.DocumentationCoverage,
		Message:               input.Message,
	}

	// Calculate health score based on overall score (0-100).
	agg.HealthScore = input.OverallScore * healthScoreMultiplier

	return agg
}

// ComputedMetrics holds all computed metric results for the comments analyzer.
type ComputedMetrics struct {
	CommentQuality        []CommentQualityData        `json:"comment_quality"        yaml:"comment_quality"`
	FunctionDocumentation []FunctionDocumentationData `json:"function_documentation" yaml:"function_documentation"`
	UndocumentedFunctions []UndocumentedFunctionData  `json:"undocumented_functions" yaml:"undocumented_functions"`
	Aggregate             AggregateData               `json:"aggregate"              yaml:"aggregate"`
}

// Analyzer name constant for MetricsOutput interface.
const analyzerNameComments = "comments"

// AnalyzerName returns the name of the analyzer.
func (m *ComputedMetrics) AnalyzerName() string {
	return analyzerNameComments
}

// ToJSON returns the metrics as a JSON-serializable object.
func (m *ComputedMetrics) ToJSON() any {
	return m
}

// ToYAML returns the metrics as a YAML-serializable object.
func (m *ComputedMetrics) ToYAML() any {
	return m
}

// ComputeAllMetrics runs all comments metrics and returns the results.
func ComputeAllMetrics(report analyze.Report) (*ComputedMetrics, error) {
	input, err := ParseReportData(report)
	if err != nil {
		return nil, err
	}

	qualityMetric := NewCommentQualityMetric()
	commentQuality := qualityMetric.Compute(input)

	docMetric := NewFunctionDocumentationMetric()
	funcDoc := docMetric.Compute(input)

	undocMetric := NewUndocumentedFunctionMetric()
	undocumented := undocMetric.Compute(input)

	aggMetric := NewAggregateMetric()
	aggregate := aggMetric.Compute(input)

	return &ComputedMetrics{
		CommentQuality:        commentQuality,
		FunctionDocumentation: funcDoc,
		UndocumentedFunctions: undocumented,
		Aggregate:             aggregate,
	}, nil
}
