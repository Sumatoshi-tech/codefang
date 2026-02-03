package complexity

import (
	"sort"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/metrics"
)

// --- Input Data Types ---

// ReportData is the parsed input data for complexity metrics computation.
type ReportData struct {
	TotalFunctions      int
	AverageComplexity   float64
	MaxComplexity       int
	TotalComplexity     int
	CognitiveComplexity int
	NestingDepth        int
	DecisionPoints      int
	Functions           []FunctionData
	Message             string
}

// FunctionData holds complexity data for a single function.
type FunctionData struct {
	Name                 string
	CyclomaticComplexity int
	CognitiveComplexity  int
	NestingDepth         int
	LinesOfCode          int
	ComplexityAssessment string
	CognitiveAssessment  string
	NestingAssessment    string
}

// ParseReportData extracts ReportData from an analyzer report.
func ParseReportData(report analyze.Report) (*ReportData, error) {
	data := &ReportData{}
	parseReportScalars(data, report)
	data.Functions = parseReportFunctions(report)

	return data, nil
}

func parseReportScalars(data *ReportData, report analyze.Report) {
	if v, ok := report["total_functions"].(int); ok {
		data.TotalFunctions = v
	}

	if v, ok := report["average_complexity"].(float64); ok {
		data.AverageComplexity = v
	}

	if v, ok := report["max_complexity"].(int); ok {
		data.MaxComplexity = v
	}

	if v, ok := report["total_complexity"].(int); ok {
		data.TotalComplexity = v
	}

	if v, ok := report["cognitive_complexity"].(int); ok {
		data.CognitiveComplexity = v
	}

	if v, ok := report["nesting_depth"].(int); ok {
		data.NestingDepth = v
	}

	if v, ok := report["decision_points"].(int); ok {
		data.DecisionPoints = v
	}

	if v, ok := report["message"].(string); ok {
		data.Message = v
	}
}

func parseReportFunctions(report analyze.Report) []FunctionData {
	functions, ok := report["functions"].([]map[string]any)
	if !ok {
		return nil
	}

	result := make([]FunctionData, 0, len(functions))

	for _, fn := range functions {
		result = append(result, parseFunctionData(fn))
	}

	return result
}

func parseFunctionData(fn map[string]any) FunctionData {
	fd := FunctionData{}

	if name, ok := fn["name"].(string); ok {
		fd.Name = name
	}

	if v, ok := fn["cyclomatic_complexity"].(int); ok {
		fd.CyclomaticComplexity = v
	}

	if v, ok := fn["cognitive_complexity"].(int); ok {
		fd.CognitiveComplexity = v
	}

	if v, ok := fn["nesting_depth"].(int); ok {
		fd.NestingDepth = v
	}

	if v, ok := fn["lines_of_code"].(int); ok {
		fd.LinesOfCode = v
	}

	if v, ok := fn["complexity_assessment"].(string); ok {
		fd.ComplexityAssessment = v
	}

	if v, ok := fn["cognitive_assessment"].(string); ok {
		fd.CognitiveAssessment = v
	}

	if v, ok := fn["nesting_assessment"].(string); ok {
		fd.NestingAssessment = v
	}

	return fd
}

// --- Output Data Types ---

// FunctionComplexityData contains detailed complexity for a function.
type FunctionComplexityData struct {
	Name                 string  `json:"name"                  yaml:"name"`
	CyclomaticComplexity int     `json:"cyclomatic_complexity" yaml:"cyclomatic_complexity"`
	CognitiveComplexity  int     `json:"cognitive_complexity"  yaml:"cognitive_complexity"`
	NestingDepth         int     `json:"nesting_depth"         yaml:"nesting_depth"`
	LinesOfCode          int     `json:"lines_of_code"         yaml:"lines_of_code"`
	ComplexityDensity    float64 `json:"complexity_density"    yaml:"complexity_density"`
	RiskLevel            string  `json:"risk_level"            yaml:"risk_level"`
}

// DistributionData contains complexity distribution counts.
type DistributionData struct {
	Simple   int `json:"simple"   yaml:"simple"`
	Moderate int `json:"moderate" yaml:"moderate"`
	Complex  int `json:"complex"  yaml:"complex"`
}

// HighRiskFunctionData identifies functions needing refactoring attention.
type HighRiskFunctionData struct {
	Name                 string   `json:"name"                  yaml:"name"`
	CyclomaticComplexity int      `json:"cyclomatic_complexity" yaml:"cyclomatic_complexity"`
	CognitiveComplexity  int      `json:"cognitive_complexity"  yaml:"cognitive_complexity"`
	RiskLevel            string   `json:"risk_level"            yaml:"risk_level"`
	Issues               []string `json:"issues"                yaml:"issues"`
}

// AggregateData contains summary statistics.
type AggregateData struct {
	TotalFunctions      int     `json:"total_functions"      yaml:"total_functions"`
	AverageComplexity   float64 `json:"average_complexity"   yaml:"average_complexity"`
	MaxComplexity       int     `json:"max_complexity"       yaml:"max_complexity"`
	TotalComplexity     int     `json:"total_complexity"     yaml:"total_complexity"`
	CognitiveComplexity int     `json:"cognitive_complexity" yaml:"cognitive_complexity"`
	NestingDepth        int     `json:"nesting_depth"        yaml:"nesting_depth"`
	DecisionPoints      int     `json:"decision_points"      yaml:"decision_points"`
	HealthScore         float64 `json:"health_score"         yaml:"health_score"`
	Message             string  `json:"message"              yaml:"message"`
}

// --- Metric Implementations ---

// FunctionComplexityMetric computes per-function complexity data.
type FunctionComplexityMetric struct {
	metrics.MetricMeta
}

// NewFunctionComplexityMetric creates the function complexity metric.
func NewFunctionComplexityMetric() *FunctionComplexityMetric {
	return &FunctionComplexityMetric{
		MetricMeta: metrics.MetricMeta{
			MetricName:        "function_complexity",
			MetricDisplayName: "Function Complexity",
			MetricDescription: "Per-function cyclomatic and cognitive complexity metrics. " +
				"Includes complexity density (complexity/LOC) and risk classification.",
			MetricType: "list",
		},
	}
}

// Complexity thresholds.
const (
	CyclomaticThresholdHigh     = 10
	CyclomaticThresholdModerate = 5
	CognitiveThresholdHigh      = 15
	CognitiveThresholdModerate  = 7
	NestingThresholdHigh        = 5
	NestingThresholdModerate    = 3

	// Risk score thresholds for classification.
	riskScoreCritical = 5
	riskScoreHigh     = 3

	// Risk priority values for sorting.
	riskPriorityCritical = 0
	riskPriorityHigh     = 1
	riskPriorityMedium   = 2
	riskPriorityDefault  = 3
)

// Compute calculates function complexity data.
func (m *FunctionComplexityMetric) Compute(input *ReportData) []FunctionComplexityData {
	result := make([]FunctionComplexityData, 0, len(input.Functions))

	for _, fn := range input.Functions {
		var density float64
		if fn.LinesOfCode > 0 {
			density = float64(fn.CyclomaticComplexity) / float64(fn.LinesOfCode)
		}

		riskLevel := classifyFunctionRisk(fn.CyclomaticComplexity, fn.CognitiveComplexity, fn.NestingDepth)

		result = append(result, FunctionComplexityData{
			Name:                 fn.Name,
			CyclomaticComplexity: fn.CyclomaticComplexity,
			CognitiveComplexity:  fn.CognitiveComplexity,
			NestingDepth:         fn.NestingDepth,
			LinesOfCode:          fn.LinesOfCode,
			ComplexityDensity:    density,
			RiskLevel:            riskLevel,
		})
	}

	// Sort by cyclomatic complexity descending
	sort.Slice(result, func(i, j int) bool {
		return result[i].CyclomaticComplexity > result[j].CyclomaticComplexity
	})

	return result
}

func classifyFunctionRisk(cyclomatic, cognitive, nesting int) string {
	score := 0

	if cyclomatic >= CyclomaticThresholdHigh {
		score += 2
	} else if cyclomatic >= CyclomaticThresholdModerate {
		score++
	}

	if cognitive >= CognitiveThresholdHigh {
		score += 2
	} else if cognitive >= CognitiveThresholdModerate {
		score++
	}

	if nesting >= NestingThresholdHigh {
		score += 2
	} else if nesting >= NestingThresholdModerate {
		score++
	}

	switch {
	case score >= riskScoreCritical:
		return "CRITICAL"
	case score >= riskScoreHigh:
		return "HIGH"
	case score >= 1:
		return "MEDIUM"
	default:
		return "LOW"
	}
}

// DistributionMetric computes complexity distribution.
type DistributionMetric struct {
	metrics.MetricMeta
}

// NewDistributionMetric creates the distribution metric.
func NewDistributionMetric() *DistributionMetric {
	return &DistributionMetric{
		MetricMeta: metrics.MetricMeta{
			MetricName:        "complexity_distribution",
			MetricDisplayName: "Complexity Distribution",
			MetricDescription: "Distribution of functions by complexity level. " +
				"Simple (1-5), Moderate (6-10), Complex (>10) cyclomatic complexity.",
			MetricType: "aggregate",
		},
	}
}

// Compute calculates complexity distribution.
func (m *DistributionMetric) Compute(input *ReportData) DistributionData {
	dist := DistributionData{}

	for _, fn := range input.Functions {
		switch {
		case fn.CyclomaticComplexity <= CyclomaticThresholdModerate:
			dist.Simple++
		case fn.CyclomaticComplexity <= CyclomaticThresholdHigh:
			dist.Moderate++
		default:
			dist.Complex++
		}
	}

	return dist
}

// HighRiskFunctionMetric identifies functions needing attention.
type HighRiskFunctionMetric struct {
	metrics.MetricMeta
}

// NewHighRiskFunctionMetric creates the high risk function metric.
func NewHighRiskFunctionMetric() *HighRiskFunctionMetric {
	return &HighRiskFunctionMetric{
		MetricMeta: metrics.MetricMeta{
			MetricName:        "high_risk_functions",
			MetricDisplayName: "High Risk Functions",
			MetricDescription: "Functions with high complexity that may need refactoring. " +
				"Identifies specific issues like high cyclomatic/cognitive complexity or deep nesting.",
			MetricType: "risk",
		},
	}
}

// Compute identifies high risk functions.
func (m *HighRiskFunctionMetric) Compute(input *ReportData) []HighRiskFunctionData {
	result := make([]HighRiskFunctionData, 0)

	for _, fn := range input.Functions {
		var issues []string

		if fn.CyclomaticComplexity >= CyclomaticThresholdHigh {
			issues = append(issues, "High cyclomatic complexity")
		}

		if fn.CognitiveComplexity >= CognitiveThresholdHigh {
			issues = append(issues, "High cognitive complexity")
		}

		if fn.NestingDepth >= NestingThresholdHigh {
			issues = append(issues, "Deep nesting")
		}

		if len(issues) == 0 {
			continue
		}

		riskLevel := classifyFunctionRisk(fn.CyclomaticComplexity, fn.CognitiveComplexity, fn.NestingDepth)

		result = append(result, HighRiskFunctionData{
			Name:                 fn.Name,
			CyclomaticComplexity: fn.CyclomaticComplexity,
			CognitiveComplexity:  fn.CognitiveComplexity,
			RiskLevel:            riskLevel,
			Issues:               issues,
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
	case "CRITICAL":
		return riskPriorityCritical
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
			MetricName:        "complexity_aggregate",
			MetricDisplayName: "Complexity Summary",
			MetricDescription: "Aggregate complexity statistics including total, average, and max complexity. " +
				"Health score indicates overall code maintainability (0-100).",
			MetricType: "aggregate",
		},
	}
}

// Health score thresholds and ranges.
const (
	healthScorePerfect      = 100.0
	healthScoreGoodBase     = 80.0
	healthScoreModerateBase = 50.0
	healthAvgLow            = 1.0
	healthAvgGood           = 3.0
	healthAvgHigh           = 7.0
)

func calculateHealthScore(avgComplexity float64) float64 {
	switch {
	case avgComplexity <= healthAvgLow:
		return healthScorePerfect
	case avgComplexity <= healthAvgGood:
		return healthScoreGoodBase + (healthAvgGood-avgComplexity)*10.0
	case avgComplexity <= healthAvgHigh:
		return healthScoreModerateBase + (healthAvgHigh-avgComplexity)*7.5
	default:
		return max(0.0, healthScoreModerateBase-(avgComplexity-healthAvgHigh)*5.0)
	}
}

// Compute calculates aggregate statistics.
func (m *AggregateMetric) Compute(input *ReportData) AggregateData {
	agg := AggregateData{
		TotalFunctions:      input.TotalFunctions,
		AverageComplexity:   input.AverageComplexity,
		MaxComplexity:       input.MaxComplexity,
		TotalComplexity:     input.TotalComplexity,
		CognitiveComplexity: input.CognitiveComplexity,
		NestingDepth:        input.NestingDepth,
		DecisionPoints:      input.DecisionPoints,
		Message:             input.Message,
	}

	// Calculate health score (0-100). Lower average complexity = higher health.
	agg.HealthScore = calculateHealthScore(input.AverageComplexity)

	return agg
}

// --- Computed Metrics ---

// ComputedMetrics holds all computed metric results for the complexity analyzer.
type ComputedMetrics struct {
	FunctionComplexity []FunctionComplexityData `json:"function_complexity" yaml:"function_complexity"`
	Distribution       DistributionData         `json:"distribution"        yaml:"distribution"`
	HighRiskFunctions  []HighRiskFunctionData   `json:"high_risk_functions" yaml:"high_risk_functions"`
	Aggregate          AggregateData            `json:"aggregate"           yaml:"aggregate"`
}

const analyzerNameComplexity = "complexity"

// AnalyzerName returns the name of the analyzer that produced these metrics.
func (m *ComputedMetrics) AnalyzerName() string {
	return analyzerNameComplexity
}

// ToJSON returns the metrics in a format suitable for JSON marshaling.
func (m *ComputedMetrics) ToJSON() any {
	return m
}

// ToYAML returns the metrics in a format suitable for YAML marshaling.
func (m *ComputedMetrics) ToYAML() any {
	return m
}

// ComputeAllMetrics runs all complexity metrics and returns the results.
func ComputeAllMetrics(report analyze.Report) (*ComputedMetrics, error) {
	input, err := ParseReportData(report)
	if err != nil {
		return nil, err
	}

	funcMetric := NewFunctionComplexityMetric()
	functionComplexity := funcMetric.Compute(input)

	distMetric := NewDistributionMetric()
	distribution := distMetric.Compute(input)

	riskMetric := NewHighRiskFunctionMetric()
	highRisk := riskMetric.Compute(input)

	aggMetric := NewAggregateMetric()
	aggregate := aggMetric.Compute(input)

	return &ComputedMetrics{
		FunctionComplexity: functionComplexity,
		Distribution:       distribution,
		HighRiskFunctions:  highRisk,
		Aggregate:          aggregate,
	}, nil
}
