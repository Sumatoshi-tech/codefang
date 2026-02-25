package halstead

import (
	"sort"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/metrics"
)

// --- Input Data Types ---.

// ReportData is the parsed input data for Halstead metrics computation.
type ReportData struct {
	TotalFunctions    int
	Volume            float64
	Difficulty        float64
	Effort            float64
	TimeToProgram     float64
	DeliveredBugs     float64
	DistinctOperators int
	DistinctOperands  int
	TotalOperators    int
	TotalOperands     int
	Vocabulary        int
	Length            int
	EstimatedLength   float64
	Functions         []FunctionData
	Message           string
}

// FunctionData holds Halstead data for a single function.
type FunctionData struct {
	Name              string
	Volume            float64
	Difficulty        float64
	Effort            float64
	TimeToProgram     float64
	DeliveredBugs     float64
	EstimatedLength   float64
	DistinctOperators int
	DistinctOperands  int
	TotalOperators    int
	TotalOperands     int
	Vocabulary        int
	Length            int
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

	if v, ok := report["volume"].(float64); ok {
		data.Volume = v
	}

	if v, ok := report["difficulty"].(float64); ok {
		data.Difficulty = v
	}

	if v, ok := report["effort"].(float64); ok {
		data.Effort = v
	}

	if v, ok := report["time_to_program"].(float64); ok {
		data.TimeToProgram = v
	}

	if v, ok := report["delivered_bugs"].(float64); ok {
		data.DeliveredBugs = v
	}

	if v, ok := report["distinct_operators"].(int); ok {
		data.DistinctOperators = v
	}

	if v, ok := report["distinct_operands"].(int); ok {
		data.DistinctOperands = v
	}

	if v, ok := report["total_operators"].(int); ok {
		data.TotalOperators = v
	}

	if v, ok := report["total_operands"].(int); ok {
		data.TotalOperands = v
	}

	if v, ok := report["vocabulary"].(int); ok {
		data.Vocabulary = v
	}

	if v, ok := report["length"].(int); ok {
		data.Length = v
	}

	if v, ok := report["estimated_length"].(float64); ok {
		data.EstimatedLength = v
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

	if v, ok := fn["volume"].(float64); ok {
		fd.Volume = v
	}

	if v, ok := fn["difficulty"].(float64); ok {
		fd.Difficulty = v
	}

	if v, ok := fn["effort"].(float64); ok {
		fd.Effort = v
	}

	if v, ok := fn["time_to_program"].(float64); ok {
		fd.TimeToProgram = v
	}

	if v, ok := fn["delivered_bugs"].(float64); ok {
		fd.DeliveredBugs = v
	}

	if v, ok := fn["distinct_operators"].(int); ok {
		fd.DistinctOperators = v
	}

	if v, ok := fn["distinct_operands"].(int); ok {
		fd.DistinctOperands = v
	}

	if v, ok := fn["total_operators"].(int); ok {
		fd.TotalOperators = v
	}

	if v, ok := fn["total_operands"].(int); ok {
		fd.TotalOperands = v
	}

	if v, ok := fn["vocabulary"].(int); ok {
		fd.Vocabulary = v
	}

	if v, ok := fn["length"].(int); ok {
		fd.Length = v
	}

	if v, ok := fn["estimated_length"].(float64); ok {
		fd.EstimatedLength = v
	}

	return fd
}

// --- Output Data Types ---.

// FunctionHalsteadData contains Halstead metrics for a function.
type FunctionHalsteadData struct {
	Name            string  `json:"name"             yaml:"name"`
	Volume          float64 `json:"volume"           yaml:"volume"`
	Difficulty      float64 `json:"difficulty"       yaml:"difficulty"`
	Effort          float64 `json:"effort"           yaml:"effort"`
	TimeToProgram   float64 `json:"time_to_program"  yaml:"time_to_program"`
	DeliveredBugs   float64 `json:"delivered_bugs"   yaml:"delivered_bugs"`
	ComplexityLevel string  `json:"complexity_level" yaml:"complexity_level"`
}

// EffortDistributionData contains effort distribution counts.
type EffortDistributionData struct {
	Low      int `json:"low"       yaml:"low"`
	Medium   int `json:"medium"    yaml:"medium"`
	High     int `json:"high"      yaml:"high"`
	VeryHigh int `json:"very_high" yaml:"very_high"`
}

// HighEffortFunctionData identifies functions with high effort.
type HighEffortFunctionData struct {
	Name          string  `json:"name"            yaml:"name"`
	Volume        float64 `json:"volume"          yaml:"volume"`
	Effort        float64 `json:"effort"          yaml:"effort"`
	TimeToProgram float64 `json:"time_to_program" yaml:"time_to_program"`
	DeliveredBugs float64 `json:"delivered_bugs"  yaml:"delivered_bugs"`
	RiskLevel     string  `json:"risk_level"      yaml:"risk_level"`
}

// AggregateData contains summary statistics.
type AggregateData struct {
	TotalFunctions    int     `json:"total_functions"    yaml:"total_functions"`
	Volume            float64 `json:"volume"             yaml:"volume"`
	Difficulty        float64 `json:"difficulty"         yaml:"difficulty"`
	Effort            float64 `json:"effort"             yaml:"effort"`
	TimeToProgram     float64 `json:"time_to_program"    yaml:"time_to_program"`
	DeliveredBugs     float64 `json:"delivered_bugs"     yaml:"delivered_bugs"`
	DistinctOperators int     `json:"distinct_operators" yaml:"distinct_operators"`
	DistinctOperands  int     `json:"distinct_operands"  yaml:"distinct_operands"`
	TotalOperators    int     `json:"total_operators"    yaml:"total_operators"`
	TotalOperands     int     `json:"total_operands"     yaml:"total_operands"`
	Vocabulary        int     `json:"vocabulary"         yaml:"vocabulary"`
	Length            int     `json:"length"             yaml:"length"`
	EstimatedLength   float64 `json:"estimated_length"   yaml:"estimated_length"`
	HealthScore       float64 `json:"health_score"       yaml:"health_score"`
	Message           string  `json:"message"            yaml:"message"`
}

// --- Metric Implementations ---.

// FunctionHalsteadMetric computes per-function Halstead data.
type FunctionHalsteadMetric struct {
	metrics.MetricMeta
}

// NewFunctionHalsteadMetric creates the function Halstead metric.
func NewFunctionHalsteadMetric() *FunctionHalsteadMetric {
	return &FunctionHalsteadMetric{
		MetricMeta: metrics.MetricMeta{
			MetricName:        "function_halstead",
			MetricDisplayName: "Function Halstead Metrics",
			MetricDescription: "Per-function Halstead complexity metrics including volume, difficulty, " +
				"effort, estimated time to program, and predicted bugs.",
			MetricType: "list",
		},
	}
}

// Volume thresholds for complexity classification.
const (
	VolumeThresholdLow    = 100
	VolumeThresholdMedium = 1000
	VolumeThresholdHigh   = 5000

	// Health score base values and multipliers.
	healthScorePerfect    = 100.0
	healthScoreHighBase   = 70.0
	healthScoreMediumBase = 30.0
	healthScoreMultiplier = 30.0
	healthScoreMultMedium = 40.0
	healthScoreDivisor    = 1000.0
	healthScoreDecay      = 10.0
)

func calculateHealthScore(avgVolume float64) float64 {
	switch {
	case avgVolume < VolumeThresholdLow:
		return healthScorePerfect

	case avgVolume < VolumeThresholdMedium:
		rangeSize := float64(VolumeThresholdMedium - VolumeThresholdLow)

		return healthScoreHighBase + (float64(VolumeThresholdMedium)-avgVolume)/rangeSize*healthScoreMultiplier

	case avgVolume < VolumeThresholdHigh:
		rangeSize := float64(VolumeThresholdHigh - VolumeThresholdMedium)

		return healthScoreMediumBase + (float64(VolumeThresholdHigh)-avgVolume)/rangeSize*healthScoreMultMedium

	default:
		excess := (avgVolume - float64(VolumeThresholdHigh)) / healthScoreDivisor * healthScoreDecay

		return max(0.0, healthScoreMediumBase-excess)
	}
}

// Compute calculates function Halstead data.
func (m *FunctionHalsteadMetric) Compute(input *ReportData) []FunctionHalsteadData {
	result := make([]FunctionHalsteadData, 0, len(input.Functions))

	for _, fn := range input.Functions {
		complexityLevel := classifyVolumeLevel(fn.Volume)

		result = append(result, FunctionHalsteadData{
			Name:            fn.Name,
			Volume:          fn.Volume,
			Difficulty:      fn.Difficulty,
			Effort:          fn.Effort,
			TimeToProgram:   fn.TimeToProgram,
			DeliveredBugs:   fn.DeliveredBugs,
			ComplexityLevel: complexityLevel,
		})
	}

	// Sort by volume descending.
	sort.Slice(result, func(i, j int) bool {
		return result[i].Volume > result[j].Volume
	})

	return result
}

func classifyVolumeLevel(volume float64) string {
	switch {
	case volume >= VolumeThresholdHigh:
		return "Very High"
	case volume >= VolumeThresholdMedium:
		return "High"
	case volume >= VolumeThresholdLow:
		return "Medium"
	default:
		return "Low"
	}
}

// EffortDistributionMetric computes effort distribution.
type EffortDistributionMetric struct {
	metrics.MetricMeta
}

// NewEffortDistributionMetric creates the distribution metric.
func NewEffortDistributionMetric() *EffortDistributionMetric {
	return &EffortDistributionMetric{
		MetricMeta: metrics.MetricMeta{
			MetricName:        "effort_distribution",
			MetricDisplayName: "Effort Distribution",
			MetricDescription: "Distribution of functions by Halstead volume/effort level. " +
				"Low (<100), Medium (100-1000), High (1000-5000), Very High (>5000).",
			MetricType: "aggregate",
		},
	}
}

// Compute calculates effort distribution.
func (m *EffortDistributionMetric) Compute(input *ReportData) EffortDistributionData {
	dist := EffortDistributionData{}

	for _, fn := range input.Functions {
		switch {
		case fn.Volume >= VolumeThresholdHigh:
			dist.VeryHigh++
		case fn.Volume >= VolumeThresholdMedium:
			dist.High++
		case fn.Volume >= VolumeThresholdLow:
			dist.Medium++
		default:
			dist.Low++
		}
	}

	return dist
}

// HighEffortFunctionMetric identifies high-effort functions.
type HighEffortFunctionMetric struct {
	metrics.MetricMeta
}

// NewHighEffortFunctionMetric creates the high effort metric.
func NewHighEffortFunctionMetric() *HighEffortFunctionMetric {
	return &HighEffortFunctionMetric{
		MetricMeta: metrics.MetricMeta{
			MetricName:        "high_effort_functions",
			MetricDisplayName: "High Effort Functions",
			MetricDescription: "Functions with high Halstead effort that may be difficult to maintain " +
				"and more likely to contain bugs.",
			MetricType: "risk",
		},
	}
}

// Compute identifies high effort functions.
func (m *HighEffortFunctionMetric) Compute(input *ReportData) []HighEffortFunctionData {
	result := make([]HighEffortFunctionData, 0)

	for _, fn := range input.Functions {
		if fn.Volume < VolumeThresholdMedium {
			continue
		}

		var riskLevel string
		if fn.Volume >= VolumeThresholdHigh {
			riskLevel = "HIGH"
		} else {
			riskLevel = "MEDIUM"
		}

		result = append(result, HighEffortFunctionData{
			Name:          fn.Name,
			Volume:        fn.Volume,
			Effort:        fn.Effort,
			TimeToProgram: fn.TimeToProgram,
			DeliveredBugs: fn.DeliveredBugs,
			RiskLevel:     riskLevel,
		})
	}

	// Sort by volume descending.
	sort.Slice(result, func(i, j int) bool {
		return result[i].Volume > result[j].Volume
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
			MetricName:        "halstead_aggregate",
			MetricDisplayName: "Halstead Summary",
			MetricDescription: "Aggregate Halstead metrics including total volume, difficulty, effort, " +
				"time to program, and estimated bugs. Health score indicates maintainability (0-100).",
			MetricType: "aggregate",
		},
	}
}

// Compute calculates aggregate statistics.
func (m *AggregateMetric) Compute(input *ReportData) AggregateData {
	agg := AggregateData{
		TotalFunctions:    input.TotalFunctions,
		Volume:            input.Volume,
		Difficulty:        input.Difficulty,
		Effort:            input.Effort,
		TimeToProgram:     input.TimeToProgram,
		DeliveredBugs:     input.DeliveredBugs,
		DistinctOperators: input.DistinctOperators,
		DistinctOperands:  input.DistinctOperands,
		TotalOperators:    input.TotalOperators,
		TotalOperands:     input.TotalOperands,
		Vocabulary:        input.Vocabulary,
		Length:            input.Length,
		EstimatedLength:   input.EstimatedLength,
		Message:           input.Message,
	}

	// Calculate health score based on average volume per function.
	if input.TotalFunctions > 0 {
		avgVolume := input.Volume / float64(input.TotalFunctions)
		agg.HealthScore = calculateHealthScore(avgVolume)
	}

	return agg
}

// --- Computed Metrics ---.

// ComputedMetrics holds all computed metric results for the Halstead analyzer.
type ComputedMetrics struct {
	FunctionHalstead    []FunctionHalsteadData   `json:"function_halstead"     yaml:"function_halstead"`
	Distribution        EffortDistributionData   `json:"distribution"          yaml:"distribution"`
	HighEffortFunctions []HighEffortFunctionData `json:"high_effort_functions" yaml:"high_effort_functions"`
	Aggregate           AggregateData            `json:"aggregate"             yaml:"aggregate"`
}

const analyzerNameHalstead = "halstead"

// AnalyzerName returns the name of the analyzer that produced these metrics.
func (m *ComputedMetrics) AnalyzerName() string {
	return analyzerNameHalstead
}

// ToJSON returns the metrics in a format suitable for JSON marshaling.
func (m *ComputedMetrics) ToJSON() any {
	return m
}

// ToYAML returns the metrics in a format suitable for YAML marshaling.
func (m *ComputedMetrics) ToYAML() any {
	return m
}

// ComputeAllMetrics runs all Halstead metrics and returns the results.
func ComputeAllMetrics(report analyze.Report) (*ComputedMetrics, error) {
	input, err := ParseReportData(report)
	if err != nil {
		return nil, err
	}

	funcMetric := NewFunctionHalsteadMetric()
	functionHalstead := funcMetric.Compute(input)

	distMetric := NewEffortDistributionMetric()
	distribution := distMetric.Compute(input)

	effortMetric := NewHighEffortFunctionMetric()
	highEffort := effortMetric.Compute(input)

	aggMetric := NewAggregateMetric()
	aggregate := aggMetric.Compute(input)

	return &ComputedMetrics{
		FunctionHalstead:    functionHalstead,
		Distribution:        distribution,
		HighEffortFunctions: highEffort,
		Aggregate:           aggregate,
	}, nil
}
