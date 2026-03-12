package common

import "github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"

// MetricResult represents a single computed metric with its metadata.
type MetricResult struct {
	Name        string // Machine-readable identifier (e.g., "typo_list").
	Display     string // Human-readable label (e.g., "Typo List").
	Description string // Short description of what this metric measures.
	Type        string // Data type hint (e.g., "list", "aggregate", "scalar").
	Value       any    // The computed metric value.
}

// MetricSet holds computed metrics for an analyzer and provides the
// AnalyzerName, ToJSON, and ToYAML methods required by the serialization
// chain in [analyze.BaseHistoryAnalyzer].
type MetricSet struct {
	analyzer string
	results  []MetricResult
}

// AnalyzerName returns the name of the analyzer that produced these metrics.
func (ms *MetricSet) AnalyzerName() string { return ms.analyzer }

// ToJSON returns a map keyed by metric name for JSON serialization.
// The structure mirrors the per-analyzer ComputedMetrics JSON tags.
func (ms *MetricSet) ToJSON() any {
	m := make(map[string]any, len(ms.results))
	for _, r := range ms.results {
		m[r.Name] = r.Value
	}

	return m
}

// ToYAML returns the same map as [MetricSet.ToJSON] for YAML serialization.
func (ms *MetricSet) ToYAML() any { return ms.ToJSON() }

// Metrics returns the underlying metric results.
func (ms *MetricSet) Metrics() []MetricResult { return ms.results }

// ComputeAllMetrics evaluates each computer function against the report
// and collects the results into a [MetricSet].
func ComputeAllMetrics(
	analyzerName string,
	computers []func(analyze.Report) MetricResult,
	report analyze.Report,
) *MetricSet {
	results := make([]MetricResult, 0, len(computers))
	for _, c := range computers {
		results = append(results, c(report))
	}

	return &MetricSet{analyzer: analyzerName, results: results}
}
