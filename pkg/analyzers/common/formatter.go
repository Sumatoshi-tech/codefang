package common

import (
	"fmt"
	"sort"
	"strings"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/jedib0t/go-pretty/v6/table"
)

// FormatConfig defines configuration for formatting
type FormatConfig struct {
	ShowProgressBars bool
	ShowTables       bool
	ShowDetails      bool
	SkipHeader       bool
	MaxItems         int
	SortBy           string
	SortOrder        string // "asc" or "desc"
}

// Formatter provides generic formatting capabilities for analysis results
type Formatter struct {
	config FormatConfig
}

// NewFormatter creates a new Formatter with configurable formatting settings
func NewFormatter(config FormatConfig) *Formatter {
	return &Formatter{
		config: config,
	}
}

// FormatReport formats an analysis report for display
func (f *Formatter) FormatReport(report analyze.Report) string {
	if report == nil {
		return "No report data available"
	}

	var parts []string

	// Add header (unless skipped)
	if !f.config.SkipHeader {
		if analyzerName, ok := report["analyzer_name"].(string); ok {
			parts = append(parts, fmt.Sprintf("=== %s ===", strings.ToUpper(analyzerName)))
		}
	}

	// Add summary
	summary := f.formatSummary(report)
	if summary != "" {
		parts = append(parts, summary)
	}

	// Add progress bars if enabled
	if f.config.ShowProgressBars {
		progressBars := f.formatProgressBars(report)
		if progressBars != "" {
			parts = append(parts, progressBars)
		}
	}

	// Add tables if enabled
	if f.config.ShowTables {
		tables := f.formatTables(report)
		if tables != "" {
			parts = append(parts, tables)
		}
	}

	// Add details if enabled
	if f.config.ShowDetails {
		details := f.formatDetails(report)
		if details != "" {
			parts = append(parts, details)
		}
	}

	return strings.Join(parts, "\n\n")
}

// formatSummary formats the summary section of a report
func (f *Formatter) formatSummary(report analyze.Report) string {
	var summary []string

	// Add message
	if message, ok := report["message"].(string); ok && message != "" {
		summary = append(summary, message)
	}

	// Add key metrics
	metrics := f.extractMetrics(report)
	if len(metrics) > 0 {
		metricLines := make([]string, 0, len(metrics))
		for key, value := range metrics {
			metricLines = append(metricLines, fmt.Sprintf("%s: %.2f", key, value))
		}
		sort.Strings(metricLines)
		summary = append(summary, strings.Join(metricLines, " | "))
	}

	return strings.Join(summary, "\n")
}

// formatProgressBars formats progress bars for numeric values
func (f *Formatter) formatProgressBars(report analyze.Report) string {
	var bars []string

	// Define count metrics that should not be shown as progress bars
	countMetrics := map[string]bool{
		"total_comments":        true,
		"good_comments":         true,
		"bad_comments":          true,
		"total_functions":       true,
		"documented_functions":  true,
		"total_comment_details": true,
	}

	// Find numeric values that could be scores (0-1 range)
	for key, value := range report {
		// Skip count metrics
		if countMetrics[key] {
			continue
		}

		if score, ok := f.toFloat(value); ok && score >= 0 && score <= 1 {
			bar := f.createProgressBar(key, score)
			if bar != "" {
				bars = append(bars, bar)
			}
		}
	}

	if len(bars) == 0 {
		return ""
	}

	return "Progress:\n" + strings.Join(bars, "\n")
}

// formatTables formats data as tables using go-pretty
func (f *Formatter) formatTables(report analyze.Report) string {
	var tables []string

	// Format collections as tables
	for key, value := range report {
		if collection, ok := value.([]map[string]interface{}); ok && len(collection) > 0 {
			tableStr := f.formatCollectionTable(key, collection)
			if tableStr != "" {
				tables = append(tables, tableStr)
			}
		}
	}

	return strings.Join(tables, "\n\n")
}

// formatDetails formats detailed information
func (f *Formatter) formatDetails(report analyze.Report) string {
	var details []string

	// Add all non-collection fields
	for key, value := range report {
		if _, ok := value.([]map[string]interface{}); !ok {
			details = append(details, fmt.Sprintf("%s: %v", key, value))
		}
	}

	if len(details) == 0 {
		return ""
	}

	sort.Strings(details)
	return "Details:\n" + strings.Join(details, "\n")
}

// formatCollectionTable formats a collection as a table using go-pretty
func (f *Formatter) formatCollectionTable(collectionKey string, collection []map[string]interface{}) string {
	if len(collection) == 0 {
		return ""
	}

	// Limit items if configured
	if f.config.MaxItems > 0 && len(collection) > f.config.MaxItems {
		collection = collection[:f.config.MaxItems]
	}

	// Sort if configured
	if f.config.SortBy != "" {
		f.sortCollection(collection, f.config.SortBy, f.config.SortOrder)
	}

	// Get all unique keys from all items
	keys := f.getCollectionKeys(collection)
	if len(keys) == 0 {
		return ""
	}

	// Create go-pretty table
	t := table.NewWriter()
	t.SetStyle(table.StyleLight)
	t.Style().Options.SeparateRows = false
	t.Style().Options.SeparateColumns = false
	t.Style().Options.DrawBorder = false
	t.Style().Options.SeparateHeader = false

	// Add header
	header := make([]interface{}, len(keys))
	for i, key := range keys {
		header[i] = key
	}
	t.AppendHeader(header)

	// Add rows
	for _, item := range collection {
		row := make([]interface{}, len(keys))
		for i, key := range keys {
			value := item[key]
			if value == nil {
				row[i] = ""
			} else {
				row[i] = fmt.Sprintf("%v", value)
			}
		}
		t.AppendRow(row)
	}

	// Add footer with count
	t.AppendFooter(table.Row{fmt.Sprintf("Total: %d items", len(collection))})

	return fmt.Sprintf("%s:\n%s", collectionKey, t.Render())
}

// createProgressBar creates a progress bar for a score
func (f *Formatter) createProgressBar(label string, score float64) string {
	const barLength = 20
	filled := int(score * barLength)
	empty := barLength - filled

	bar := strings.Repeat("█", filled) + strings.Repeat("░", empty)
	percentage := score * 100

	status := "🔴 Poor"
	if score >= 0.8 {
		status = "🟢 Good"
	} else if score >= 0.6 {
		status = "🟡 Fair"
	}

	return fmt.Sprintf("%s: [%s] %.1f%% %s", label, bar, percentage, status)
}

// extractMetrics extracts numeric metrics from a report
func (f *Formatter) extractMetrics(report analyze.Report) map[string]float64 {
	metrics := make(map[string]float64)

	for key, value := range report {
		if score, ok := f.toFloat(value); ok {
			metrics[key] = score
		}
	}

	return metrics
}

// toFloat safely converts a value to float64
func (f *Formatter) toFloat(value interface{}) (float64, bool) {
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

// getCollectionKeys gets all unique keys from a collection
func (f *Formatter) getCollectionKeys(collection []map[string]interface{}) []string {
	keySet := make(map[string]bool)

	for _, item := range collection {
		for key := range item {
			keySet[key] = true
		}
	}

	keys := make([]string, 0, len(keySet))
	for key := range keySet {
		keys = append(keys, key)
	}

	sort.Strings(keys)
	return keys
}

// sortCollection sorts a collection by a specific key
func (f *Formatter) sortCollection(collection []map[string]interface{}, sortBy, sortOrder string) {
	sort.Slice(collection, func(i, j int) bool {
		valI := collection[i][sortBy]
		valJ := collection[j][sortBy]

		// Convert to comparable values
		compI := f.toComparable(valI)
		compJ := f.toComparable(valJ)

		if sortOrder == "desc" {
			return compI > compJ
		}
		return compI < compJ
	})
}

// toComparable converts a value to a comparable type for sorting
func (f *Formatter) toComparable(value interface{}) float64 {
	switch v := value.(type) {
	case float64:
		return v
	case int:
		return float64(v)
	case int32:
		return float64(v)
	case int64:
		return float64(v)
	case string:
		return float64(len(v)) // Sort strings by length as fallback
	default:
		return 0
	}
}
