package cohesion

import (
	"sort"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/metrics"
)

// --- Input Data Types ---.

// ReportData is the parsed input data for cohesion metrics computation.
type ReportData struct {
	TotalFunctions   int
	LCOM             float64
	CohesionScore    float64
	FunctionCohesion float64
	Functions        []FunctionData
	Message          string
}

// FunctionData holds cohesion data for a single function.
type FunctionData struct {
	Name     string
	Cohesion float64
}

// ParseReportData extracts ReportData from an analyzer report.
func ParseReportData(report analyze.Report) (*ReportData, error) {
	data := &ReportData{}

	if v, ok := report["total_functions"].(int); ok {
		data.TotalFunctions = v
	}

	if v, ok := report["lcom"].(float64); ok {
		data.LCOM = v
	}

	if v, ok := report["cohesion_score"].(float64); ok {
		data.CohesionScore = v
	}

	if v, ok := report["function_cohesion"].(float64); ok {
		data.FunctionCohesion = v
	}

	if v, ok := report["message"].(string); ok {
		data.Message = v
	}

	// Parse functions.
	if functions, ok := report["functions"].([]map[string]any); ok {
		data.Functions = make([]FunctionData, 0, len(functions))

		for _, fn := range functions {
			fd := FunctionData{}

			if name, nameOK := fn["name"].(string); nameOK {
				fd.Name = name
			}

			if v, vOK := fn["cohesion"].(float64); vOK {
				fd.Cohesion = v
			}

			data.Functions = append(data.Functions, fd)
		}
	}

	return data, nil
}

// --- Output Data Types ---.

// FunctionCohesionData contains cohesion data for a function.
type FunctionCohesionData struct {
	Name         string  `json:"name"          yaml:"name"`
	Cohesion     float64 `json:"cohesion"      yaml:"cohesion"`
	QualityLevel string  `json:"quality_level" yaml:"quality_level"`
}

// DistributionData contains cohesion distribution counts.
type DistributionData struct {
	Excellent int `json:"excellent" yaml:"excellent"`
	Good      int `json:"good"      yaml:"good"`
	Fair      int `json:"fair"      yaml:"fair"`
	Poor      int `json:"poor"      yaml:"poor"`
}

// LowCohesionFunctionData identifies functions with poor cohesion.
type LowCohesionFunctionData struct {
	Name           string  `json:"name"           yaml:"name"`
	Cohesion       float64 `json:"cohesion"       yaml:"cohesion"`
	RiskLevel      string  `json:"risk_level"     yaml:"risk_level"`
	Recommendation string  `json:"recommendation" yaml:"recommendation"`
}

// AggregateData contains summary statistics.
type AggregateData struct {
	TotalFunctions   int     `json:"total_functions"   yaml:"total_functions"`
	LCOM             float64 `json:"lcom"              yaml:"lcom"`
	CohesionScore    float64 `json:"cohesion_score"    yaml:"cohesion_score"`
	FunctionCohesion float64 `json:"function_cohesion" yaml:"function_cohesion"`
	HealthScore      float64 `json:"health_score"      yaml:"health_score"`
	Message          string  `json:"message"           yaml:"message"`
}

// --- Metric Implementations ---.

// FunctionCohesionMetric computes per-function cohesion data.
type FunctionCohesionMetric struct {
	metrics.MetricMeta
}

// NewFunctionCohesionMetric creates the function cohesion metric.
func NewFunctionCohesionMetric() *FunctionCohesionMetric {
	return &FunctionCohesionMetric{
		MetricMeta: metrics.MetricMeta{
			MetricName:        "function_cohesion",
			MetricDisplayName: "Function Cohesion",
			MetricDescription: "Per-function cohesion scores measuring how well variables are shared " +
				"across methods. Higher scores (closer to 1.0) indicate better cohesion.",
			MetricType: "list",
		},
	}
}

// Cohesion quality thresholds.
const (
	CohesionThresholdExcellent = 0.8
	CohesionThresholdGood      = 0.6
	CohesionThresholdFair      = 0.3

	// Health score multiplier (converts 0-1 score to 0-100).
	healthScoreMultiplier = 100
)

// Compute calculates function cohesion data.
func (m *FunctionCohesionMetric) Compute(input *ReportData) []FunctionCohesionData {
	result := make([]FunctionCohesionData, 0, len(input.Functions))

	for _, fn := range input.Functions {
		qualityLevel := classifyCohesionQuality(fn.Cohesion)

		result = append(result, FunctionCohesionData{
			Name:         fn.Name,
			Cohesion:     fn.Cohesion,
			QualityLevel: qualityLevel,
		})
	}

	// Sort by cohesion ascending (worst first).
	sort.Slice(result, func(i, j int) bool {
		return result[i].Cohesion < result[j].Cohesion
	})

	return result
}

func classifyCohesionQuality(cohesion float64) string {
	switch {
	case cohesion >= CohesionThresholdExcellent:
		return "Excellent"
	case cohesion >= CohesionThresholdGood:
		return "Good"
	case cohesion >= CohesionThresholdFair:
		return "Fair"
	default:
		return "Poor"
	}
}

// DistributionMetric computes cohesion distribution.
type DistributionMetric struct {
	metrics.MetricMeta
}

// NewDistributionMetric creates the distribution metric.
func NewDistributionMetric() *DistributionMetric {
	return &DistributionMetric{
		MetricMeta: metrics.MetricMeta{
			MetricName:        "cohesion_distribution",
			MetricDisplayName: "Cohesion Distribution",
			MetricDescription: "Distribution of functions by cohesion quality level. " +
				"Excellent (>=0.8), Good (>=0.6), Fair (>=0.3), Poor (<0.3).",
			MetricType: "aggregate",
		},
	}
}

// Compute calculates cohesion distribution.
func (m *DistributionMetric) Compute(input *ReportData) DistributionData {
	dist := DistributionData{}

	for _, fn := range input.Functions {
		switch {
		case fn.Cohesion >= CohesionThresholdExcellent:
			dist.Excellent++
		case fn.Cohesion >= CohesionThresholdGood:
			dist.Good++
		case fn.Cohesion >= CohesionThresholdFair:
			dist.Fair++
		default:
			dist.Poor++
		}
	}

	return dist
}

// LowCohesionFunctionMetric identifies functions needing attention.
type LowCohesionFunctionMetric struct {
	metrics.MetricMeta
}

// NewLowCohesionFunctionMetric creates the low cohesion metric.
func NewLowCohesionFunctionMetric() *LowCohesionFunctionMetric {
	return &LowCohesionFunctionMetric{
		MetricMeta: metrics.MetricMeta{
			MetricName:        "low_cohesion_functions",
			MetricDisplayName: "Low Cohesion Functions",
			MetricDescription: "Functions with poor cohesion that may benefit from refactoring. " +
				"Low cohesion often indicates a function is doing too many unrelated things.",
			MetricType: "risk",
		},
	}
}

// Compute identifies low cohesion functions.
func (m *LowCohesionFunctionMetric) Compute(input *ReportData) []LowCohesionFunctionData {
	result := make([]LowCohesionFunctionData, 0)

	for _, fn := range input.Functions {
		if fn.Cohesion >= CohesionThresholdGood {
			continue
		}

		var riskLevel, recommendation string

		if fn.Cohesion < CohesionThresholdFair {
			riskLevel = "HIGH"
			recommendation = "Consider splitting into multiple focused functions"
		} else {
			riskLevel = "MEDIUM"
			recommendation = "Review function responsibilities for possible separation"
		}

		result = append(result, LowCohesionFunctionData{
			Name:           fn.Name,
			Cohesion:       fn.Cohesion,
			RiskLevel:      riskLevel,
			Recommendation: recommendation,
		})
	}

	// Sort by cohesion ascending (worst first).
	sort.Slice(result, func(i, j int) bool {
		return result[i].Cohesion < result[j].Cohesion
	})

	return result
}

// AggregateMetric computes summary statistics.
type AggregateMetric struct {
	metrics.MetricMeta
}

// NewAggregateMetric creates the aggregate metric.
func NewAggregateMetric() *AggregateMetric {
	return &AggregateMetric{
		MetricMeta: metrics.MetricMeta{
			MetricName:        "cohesion_aggregate",
			MetricDisplayName: "Cohesion Summary",
			MetricDescription: "Aggregate cohesion statistics including LCOM (Lack of Cohesion of Methods) " +
				"and normalized cohesion scores. Health score indicates overall design quality (0-100).",
			MetricType: "aggregate",
		},
	}
}

// Compute calculates aggregate statistics.
func (m *AggregateMetric) Compute(input *ReportData) AggregateData {
	agg := AggregateData{
		TotalFunctions:   input.TotalFunctions,
		LCOM:             input.LCOM,
		CohesionScore:    input.CohesionScore,
		FunctionCohesion: input.FunctionCohesion,
		Message:          input.Message,
	}

	// Calculate health score based on cohesion score (0-100).
	agg.HealthScore = input.CohesionScore * healthScoreMultiplier

	return agg
}

// --- Computed Metrics ---.

// ComputedMetrics holds all computed metric results for the cohesion analyzer.
type ComputedMetrics struct {
	FunctionCohesion     []FunctionCohesionData    `json:"function_cohesion"      yaml:"function_cohesion"`
	Distribution         DistributionData          `json:"distribution"           yaml:"distribution"`
	LowCohesionFunctions []LowCohesionFunctionData `json:"low_cohesion_functions" yaml:"low_cohesion_functions"`
	Aggregate            AggregateData             `json:"aggregate"              yaml:"aggregate"`
}

const analyzerNameCohesion = "cohesion"

// AnalyzerName returns the name of the analyzer that produced these metrics.
func (m *ComputedMetrics) AnalyzerName() string {
	return analyzerNameCohesion
}

// ToJSON returns the metrics in a format suitable for JSON marshaling.
func (m *ComputedMetrics) ToJSON() any {
	return m
}

// ToYAML returns the metrics in a format suitable for YAML marshaling.
func (m *ComputedMetrics) ToYAML() any {
	return m
}

// ComputeAllMetrics runs all cohesion metrics and returns the results.
func ComputeAllMetrics(report analyze.Report) (*ComputedMetrics, error) {
	input, err := ParseReportData(report)
	if err != nil {
		return nil, err
	}

	funcMetric := NewFunctionCohesionMetric()
	functionCohesion := funcMetric.Compute(input)

	distMetric := NewDistributionMetric()
	distribution := distMetric.Compute(input)

	lowMetric := NewLowCohesionFunctionMetric()
	lowCohesion := lowMetric.Compute(input)

	aggMetric := NewAggregateMetric()
	aggregate := aggMetric.Compute(input)

	return &ComputedMetrics{
		FunctionCohesion:     functionCohesion,
		Distribution:         distribution,
		LowCohesionFunctions: lowCohesion,
		Aggregate:            aggregate,
	}, nil
}
