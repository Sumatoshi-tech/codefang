package common

import (
	"encoding/json"
	"fmt"
	"sort"
	"strings"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
)

// ReportConfig defines configuration for report generation
type ReportConfig struct {
	Format         string // "text", "json", "summary"
	IncludeDetails bool
	SortBy         string
	SortOrder      string
	MaxItems       int
	MetricKeys     []string // Configurable metric keys to extract
	CountKeys      []string // Configurable count keys to extract
}

// Reporter provides generic reporting capabilities for analysis results
type Reporter struct {
	config    ReportConfig
	formatter *Formatter
}

// NewReporter creates a new Reporter with configurable reporting settings
func NewReporter(config ReportConfig) *Reporter {
	formatConfig := FormatConfig{
		ShowProgressBars: config.Format == "text",
		ShowTables:       config.Format == "text" && config.IncludeDetails,
		ShowDetails:      config.IncludeDetails,
		MaxItems:         config.MaxItems,
		SortBy:           config.SortBy,
		SortOrder:        config.SortOrder,
	}

	return &Reporter{
		config:    config,
		formatter: NewFormatter(formatConfig),
	}
}

// GenerateReport generates a report in the specified format
func (r *Reporter) GenerateReport(report analyze.Report) (string, error) {
	switch r.config.Format {
	case "text":
		return r.generateTextReport(report), nil
	case "json":
		return r.generateJSONReport(report)
	case "summary":
		return r.generateSummaryReport(report), nil
	default:
		return r.generateTextReport(report), nil
	}
}

// generateTextReport generates a human-readable text report
func (r *Reporter) generateTextReport(report analyze.Report) string {
	return r.formatter.FormatReport(report)
}

// generateJSONReport generates a JSON report
func (r *Reporter) generateJSONReport(report analyze.Report) (string, error) {
	jsonData, err := json.MarshalIndent(report, "", "  ")
	if err != nil {
		return "", fmt.Errorf("failed to marshal report to JSON: %w", err)
	}
	return string(jsonData), nil
}

// generateSummaryReport generates a concise summary report
func (r *Reporter) generateSummaryReport(report analyze.Report) string {
	if report == nil {
		return "No report data available"
	}

	var summary []string

	// Add analyzer name
	if analyzerName, ok := report["analyzer_name"].(string); ok {
		summary = append(summary, fmt.Sprintf("Analyzer: %s", analyzerName))
	}

	// Add message
	if message, ok := report["message"].(string); ok && message != "" {
		summary = append(summary, fmt.Sprintf("Status: %s", message))
	}

	// Add key metrics
	metrics := r.extractKeyMetrics(report)
	if len(metrics) > 0 {
		metricLines := make([]string, 0, len(metrics))
		for key, value := range metrics {
			metricLines = append(metricLines, fmt.Sprintf("%s: %.2f", key, value))
		}
		sort.Strings(metricLines)
		summary = append(summary, fmt.Sprintf("Metrics: %s", strings.Join(metricLines, ", ")))
	}

	// Add item counts
	counts := r.extractCounts(report)
	if len(counts) > 0 {
		countLines := make([]string, 0, len(counts))
		for key, value := range counts {
			countLines = append(countLines, fmt.Sprintf("%s: %d", key, value))
		}
		sort.Strings(countLines)
		summary = append(summary, fmt.Sprintf("Counts: %s", strings.Join(countLines, ", ")))
	}

	return strings.Join(summary, " | ")
}

// extractKeyMetrics extracts key numeric metrics from a report
func (r *Reporter) extractKeyMetrics(report analyze.Report) map[string]float64 {
	metrics := make(map[string]float64)

	// Use configured metric keys if provided, otherwise extract all numeric values
	if len(r.config.MetricKeys) > 0 {
		for _, key := range r.config.MetricKeys {
			if value, exists := report[key]; exists {
				if score, ok := r.toFloat(value); ok {
					metrics[key] = score
				}
			}
		}
	} else {
		// Extract all numeric values as metrics
		for key, value := range report {
			if score, ok := r.toFloat(value); ok {
				metrics[key] = score
			}
		}
	}

	return metrics
}

// extractCounts extracts count metrics from a report
func (r *Reporter) extractCounts(report analyze.Report) map[string]int {
	counts := make(map[string]int)

	// Use configured count keys if provided, otherwise extract all integer values
	if len(r.config.CountKeys) > 0 {
		for _, key := range r.config.CountKeys {
			if value, exists := report[key]; exists {
				if count, ok := r.toInt(value); ok {
					counts[key] = count
				}
			}
		}
	} else {
		// Extract all integer values as counts
		for key, value := range report {
			if count, ok := r.toInt(value); ok {
				counts[key] = count
			}
		}
	}

	return counts
}

// toFloat safely converts a value to float64
func (r *Reporter) toFloat(value interface{}) (float64, bool) {
	switch v := value.(type) {
	case float64:
		return v, true
	case int:
		return float64(v), true
	case int32:
		return float64(v), true
	case int64:
		return float64(v), true
	default:
		return 0, false
	}
}

// toInt safely converts a value to int
func (r *Reporter) toInt(value interface{}) (int, bool) {
	switch v := value.(type) {
	case int:
		return v, true
	case int32:
		return int(v), true
	case int64:
		return int(v), true
	case float64:
		return int(v), true
	default:
		return 0, false
	}
}

// GenerateComparisonReport generates a comparison report between multiple reports
func (r *Reporter) GenerateComparisonReport(reports map[string]analyze.Report) (string, error) {
	if len(reports) == 0 {
		return "No reports to compare", nil
	}

	switch r.config.Format {
	case "text":
		return r.generateTextComparisonReport(reports), nil
	case "json":
		return r.generateJSONComparisonReport(reports)
	case "summary":
		return r.generateSummaryComparisonReport(reports), nil
	default:
		return r.generateTextComparisonReport(reports), nil
	}
}

// generateTextComparisonReport generates a text comparison report
func (r *Reporter) generateTextComparisonReport(reports map[string]analyze.Report) string {
	parts := make([]string, 0, len(reports)+2)

	parts = append(parts, "=== COMPARISON REPORT ===")

	// Compare key metrics across reports
	comparison := r.compareMetrics(reports)
	if comparison != "" {
		parts = append(parts, comparison)
	}

	// Show individual reports
	for name, report := range reports {
		parts = append(parts, fmt.Sprintf("\n--- %s ---", name))
		parts = append(parts, r.formatter.FormatReport(report))
	}

	return strings.Join(parts, "\n")
}

// generateJSONComparisonReport generates a JSON comparison report
func (r *Reporter) generateJSONComparisonReport(reports map[string]analyze.Report) (string, error) {
	comparisonData := map[string]interface{}{
		"comparison": r.compareMetricsData(reports),
		"reports":    reports,
	}

	jsonData, err := json.MarshalIndent(comparisonData, "", "  ")
	if err != nil {
		return "", fmt.Errorf("failed to marshal comparison report to JSON: %w", err)
	}
	return string(jsonData), nil
}

// generateSummaryComparisonReport generates a summary comparison report
func (r *Reporter) generateSummaryComparisonReport(reports map[string]analyze.Report) string {
	parts := make([]string, 0, len(reports)+2)

	parts = append(parts, "Comparison Summary:")

	// Compare key metrics
	comparison := r.compareMetrics(reports)
	if comparison != "" {
		parts = append(parts, comparison)
	}

	// Show summary for each report
	for name, report := range reports {
		summary := r.generateSummaryReport(report)
		parts = append(parts, fmt.Sprintf("%s: %s", name, summary))
	}

	return strings.Join(parts, "\n")
}

// compareMetrics compares metrics across multiple reports
func (r *Reporter) compareMetrics(reports map[string]analyze.Report) string {
	if len(reports) < 2 {
		return ""
	}

	var parts []string

	// Use configured metric keys if provided, otherwise compare all numeric values
	metricKeys := r.config.MetricKeys
	if len(metricKeys) == 0 {
		// Extract all unique metric keys from all reports
		keySet := make(map[string]bool)
		for _, report := range reports {
			for key, value := range report {
				if _, ok := r.toFloat(value); ok {
					keySet[key] = true
				}
			}
		}
		for key := range keySet {
			metricKeys = append(metricKeys, key)
		}
		sort.Strings(metricKeys)
	}

	for _, metricKey := range metricKeys {
		values := make(map[string]float64)
		hasValues := false

		for name, report := range reports {
			if value, exists := report[metricKey]; exists {
				if score, ok := r.toFloat(value); ok {
					values[name] = score
					hasValues = true
				}
			}
		}

		if hasValues {
			comparison := r.formatMetricComparison(metricKey, values)
			if comparison != "" {
				parts = append(parts, comparison)
			}
		}
	}

	return strings.Join(parts, "\n")
}

// compareMetricsData returns comparison data for JSON output
func (r *Reporter) compareMetricsData(reports map[string]analyze.Report) map[string]interface{} {
	comparison := make(map[string]interface{})

	// Use configured metric keys if provided, otherwise compare all numeric values
	metricKeys := r.config.MetricKeys
	if len(metricKeys) == 0 {
		// Extract all unique metric keys from all reports
		keySet := make(map[string]bool)
		for _, report := range reports {
			for key, value := range report {
				if _, ok := r.toFloat(value); ok {
					keySet[key] = true
				}
			}
		}
		for key := range keySet {
			metricKeys = append(metricKeys, key)
		}
		sort.Strings(metricKeys)
	}

	for _, metricKey := range metricKeys {
		values := make(map[string]float64)
		hasValues := false

		for name, report := range reports {
			if value, exists := report[metricKey]; exists {
				if score, ok := r.toFloat(value); ok {
					values[name] = score
					hasValues = true
				}
			}
		}

		if hasValues {
			comparison[metricKey] = values
		}
	}

	return comparison
}

// formatMetricComparison formats a metric comparison
func (r *Reporter) formatMetricComparison(metricKey string, values map[string]float64) string {
	if len(values) == 0 {
		return ""
	}

	lines := make([]string, 0, len(values)+1)
	lines = append(lines, fmt.Sprintf("%s:", metricKey))

	// Sort by value (descending)
	type kv struct {
		Key   string
		Value float64
	}

	sorted := make([]kv, 0, len(values))
	for k, v := range values {
		sorted = append(sorted, kv{k, v})
	}

	sort.Slice(sorted, func(i, j int) bool {
		return sorted[i].Value > sorted[j].Value
	})

	for _, kv := range sorted {
		lines = append(lines, fmt.Sprintf("  %s: %.3f", kv.Key, kv.Value))
	}

	return strings.Join(lines, "\n")
}
