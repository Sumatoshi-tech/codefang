package common //nolint:testpackage // testing internal implementation.

import (
	"strings"
	"testing"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
)

func TestFormatter_CompactTableFormatting(t *testing.T) {
	t.Parallel()

	config := FormatConfig{
		ShowTables:  true,
		ShowDetails: false,
		MaxItems:    10,
	}

	formatter := NewFormatter(config)

	// Create a test report with collection data.
	report := analyze.Report{
		"analyzer_name": "test_analyzer",
		"message":       "Test analysis completed",
		"test_data": []map[string]any{
			{"name": "item1", "value": 10, "score": 0.85},
			{"name": "item2", "value": 20, "score": 0.92},
			{"name": "item3", "value": 15, "score": 0.78},
		},
	}

	result := formatter.FormatReport(report)

	// Verify the result contains the table.
	if !strings.Contains(result, "test_data:") {
		t.Error("Expected table to be included in formatted report")
	}

	// Verify the table is compact (no borders, minimal spacing).
	if strings.Contains(result, "│") {
		t.Error("Expected compact table without border characters")
	}

	// Verify data is present.
	if !strings.Contains(result, "item1") || !strings.Contains(result, "item2") || !strings.Contains(result, "item3") {
		t.Error("Expected table to contain test data items")
	}

	// Verify footer is present (check for any footer text).
	if !strings.Contains(result, "TOTAL:") {
		t.Error("Expected table footer with item count")
	}
}

func TestNewFormatter(t *testing.T) {
	t.Parallel()

	config := FormatConfig{
		ShowProgressBars: true,
		ShowTables:       true,
		ShowDetails:      true,
		SkipHeader:       false,
		MaxItems:         5,
		SortBy:           "score",
		SortOrder:        "desc",
	}

	formatter := NewFormatter(config)
	if formatter == nil {
		t.Fatal("NewFormatter returned nil")
	}

	if !formatter.config.ShowProgressBars {
		t.Error("expected ShowProgressBars to be true")
	}
}

func TestFormatter_FormatReport_NilReport(t *testing.T) {
	t.Parallel()

	formatter := NewFormatter(FormatConfig{})

	result := formatter.FormatReport(nil)
	if result != "No report data available" {
		t.Errorf("expected 'No report data available', got '%s'", result)
	}
}

func TestFormatter_FormatReport_WithHeader(t *testing.T) {
	t.Parallel()

	formatter := NewFormatter(FormatConfig{SkipHeader: false})
	report := analyze.Report{
		"analyzer_name": "test_analyzer",
	}

	result := formatter.FormatReport(report)
	if !strings.Contains(result, "=== TEST_ANALYZER ===") {
		t.Error("expected header to be present")
	}
}

func TestFormatter_FormatReport_SkipHeader(t *testing.T) {
	t.Parallel()

	formatter := NewFormatter(FormatConfig{SkipHeader: true})
	report := analyze.Report{
		"analyzer_name": "test_analyzer",
	}

	result := formatter.FormatReport(report)
	if strings.Contains(result, "===") {
		t.Error("expected header to be skipped")
	}
}

func TestFormatter_FormatReport_WithProgressBars(t *testing.T) {
	t.Parallel()

	formatter := NewFormatter(FormatConfig{ShowProgressBars: true})
	report := analyze.Report{
		"score": 0.85,
	}

	result := formatter.FormatReport(report)
	if !strings.Contains(result, "Progress:") {
		t.Error("expected progress bars section")
	}

	if !strings.Contains(result, "█") {
		t.Error("expected filled bar characters")
	}
}

func TestFormatter_FormatReport_WithDetails(t *testing.T) {
	t.Parallel()

	formatter := NewFormatter(FormatConfig{ShowDetails: true})
	report := analyze.Report{
		"key1": "value1",
		"key2": 42,
	}

	result := formatter.FormatReport(report)
	if !strings.Contains(result, "Details:") {
		t.Error("expected details section")
	}

	if !strings.Contains(result, "key1: value1") {
		t.Error("expected key1 in details")
	}
}

func TestFormatter_formatSummary(t *testing.T) {
	t.Parallel()

	formatter := NewFormatter(FormatConfig{})
	report := analyze.Report{
		"message": "Analysis complete",
		"score":   0.85,
		"avg":     0.75,
	}

	result := formatter.formatSummary(report)
	if !strings.Contains(result, "Analysis complete") {
		t.Error("expected message in summary")
	}
}

func TestFormatter_formatProgressBars_CountMetricsExcluded(t *testing.T) {
	t.Parallel()

	formatter := NewFormatter(FormatConfig{ShowProgressBars: true})
	report := analyze.Report{
		"total_comments": 0.5, // Should be excluded (count metric).
		"score":          0.8, // Should be included.
	}

	result := formatter.formatProgressBars(report)
	if strings.Contains(result, "total_comments") {
		t.Error("count metrics should be excluded from progress bars")
	}
}

func TestFormatter_formatProgressBars_EmptyOnNoScores(t *testing.T) {
	t.Parallel()

	formatter := NewFormatter(FormatConfig{})
	report := analyze.Report{
		"name":  "test",
		"count": 100,
	}

	result := formatter.formatProgressBars(report)
	if result != "" {
		t.Errorf("expected empty string for non-score values, got '%s'", result)
	}
}

func TestFormatter_formatDetails(t *testing.T) {
	t.Parallel()

	formatter := NewFormatter(FormatConfig{ShowDetails: true})
	report := analyze.Report{
		"field1": "value1",
		"field2": 42,
		"collection": []map[string]any{
			{"name": "item1"},
		},
	}

	result := formatter.formatDetails(report)
	if !strings.Contains(result, "field1: value1") {
		t.Error("expected field1 in details")
	}

	if !strings.Contains(result, "field2: 42") {
		t.Error("expected field2 in details")
	}
	// Collection should not appear in details.
	if strings.Contains(result, "collection:") {
		t.Error("collections should not appear in details")
	}
}

func TestFormatter_formatDetails_Empty(t *testing.T) {
	t.Parallel()

	formatter := NewFormatter(FormatConfig{})
	report := analyze.Report{
		"items": []map[string]any{{"name": "test"}},
	}

	result := formatter.formatDetails(report)
	if result != "" {
		t.Errorf("expected empty details for collection-only report, got '%s'", result)
	}
}

func TestFormatter_formatCollectionTable_Empty(t *testing.T) {
	t.Parallel()

	formatter := NewFormatter(FormatConfig{})

	result := formatter.formatCollectionTable("items", []map[string]any{})
	if result != "" {
		t.Error("expected empty string for empty collection")
	}
}

func TestFormatter_formatCollectionTable_MaxItems(t *testing.T) {
	t.Parallel()

	formatter := NewFormatter(FormatConfig{MaxItems: 2})
	collection := []map[string]any{
		{"name": "item1"},
		{"name": "item2"},
		{"name": "item3"},
	}
	result := formatter.formatCollectionTable("items", collection)
	// Should only show 2 items (item3 should NOT appear).
	if strings.Contains(result, "item3") {
		t.Error("expected max 2 items in table, but item3 is present")
	}
	// Item1 and item2 should be present.
	if !strings.Contains(result, "item1") || !strings.Contains(result, "item2") {
		t.Error("expected item1 and item2 to be present")
	}
}

func TestFormatter_formatCollectionTable_WithSorting(t *testing.T) {
	t.Parallel()

	formatter := NewFormatter(FormatConfig{SortBy: "value", SortOrder: "desc"})
	collection := []map[string]any{
		{"name": "item1", "value": 10},
		{"name": "item2", "value": 30},
		{"name": "item3", "value": 20},
	}
	result := formatter.formatCollectionTable("items", collection)
	// Item2 should come first (highest value).
	idx1 := strings.Index(result, "item2")

	idx2 := strings.Index(result, "item1")
	if idx1 > idx2 {
		t.Error("expected descending sort by value")
	}
}

func TestFormatter_formatCollectionTable_NilValue(t *testing.T) {
	t.Parallel()

	formatter := NewFormatter(FormatConfig{ShowTables: true})
	collection := []map[string]any{
		{"name": "item1", "value": nil},
	}

	result := formatter.formatCollectionTable("items", collection)
	if result == "" {
		t.Error("expected table even with nil value")
	}
}

func TestFormatter_createProgressBar(t *testing.T) {
	t.Parallel()

	formatter := NewFormatter(FormatConfig{})

	// Test good score (>= 0.8).
	bar := formatter.createProgressBar("score", 0.9)
	if !strings.Contains(bar, "🟢 Good") {
		t.Errorf("expected Good status for 0.9, got '%s'", bar)
	}

	// Test fair score (>= 0.6 and < 0.8).
	bar = formatter.createProgressBar("score", 0.7)
	if !strings.Contains(bar, "🟡 Fair") {
		t.Errorf("expected Fair status for 0.7, got '%s'", bar)
	}

	// Test poor score (< 0.6).
	bar = formatter.createProgressBar("score", 0.4)
	if !strings.Contains(bar, "🔴 Poor") {
		t.Errorf("expected Poor status for 0.4, got '%s'", bar)
	}
}

func TestFormatter_extractMetrics(t *testing.T) {
	t.Parallel()

	formatter := NewFormatter(FormatConfig{})
	report := analyze.Report{
		"score": 0.85,
		"count": 10,
		"name":  "test", // Non-numeric.
		"avg":   0.75,
	}

	metrics := formatter.extractMetrics(report)
	if len(metrics) != 3 {
		t.Errorf("expected 3 numeric metrics, got %d", len(metrics))
	}

	if metrics["score"] != 0.85 {
		t.Error("expected score metric")
	}
}

func TestFormatter_toFloat(t *testing.T) { //nolint:tparallel // parallel test pattern is intentional.
	t.Parallel()

	formatter := NewFormatter(FormatConfig{})

	tests := []struct {
		value    any
		name     string
		expected float64
		ok       bool
	}{
		{3.14, "float64", 3.14, true},
		{42, "int", 42.0, true},
		{int32(10), "int32", 10.0, true},
		{int64(100), "int64", 100.0, true},
		{"invalid", "string", 0, false},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result, ok := formatter.toFloat(tt.value)
			if ok != tt.ok || (ok && result != tt.expected) {
				t.Errorf("toFloat(%v) = (%f, %v), want (%f, %v)", tt.value, result, ok, tt.expected, tt.ok)
			}
		})
	}
}

func TestFormatter_getCollectionKeys(t *testing.T) {
	t.Parallel()

	formatter := NewFormatter(FormatConfig{})
	collection := []map[string]any{
		{"name": "item1", "value": 10},
		{"name": "item2", "score": 0.5},
	}

	keys := formatter.getCollectionKeys(collection)
	if len(keys) != 3 { // Name, value, score.
		t.Errorf("expected 3 unique keys, got %d", len(keys))
	}
}

func TestFormatter_getCollectionKeys_Empty(t *testing.T) {
	t.Parallel()

	formatter := NewFormatter(FormatConfig{})

	keys := formatter.getCollectionKeys([]map[string]any{})
	if len(keys) != 0 {
		t.Error("expected no keys for empty collection")
	}
}

func TestFormatter_sortCollection(t *testing.T) {
	t.Parallel()

	formatter := NewFormatter(FormatConfig{})

	// Test ascending sort.
	collection := []map[string]any{
		{"name": "c", "value": 30},
		{"name": "a", "value": 10},
		{"name": "b", "value": 20},
	}
	formatter.sortCollection(collection, "value", "asc")

	if collection[0]["name"] != "a" {
		t.Error("expected 'a' first after ascending sort")
	}

	// Test descending sort.
	collection = []map[string]any{
		{"name": "c", "value": 30},
		{"name": "a", "value": 10},
		{"name": "b", "value": 20},
	}
	formatter.sortCollection(collection, "value", "desc")

	if collection[0]["name"] != "c" {
		t.Error("expected 'c' first after descending sort")
	}
}

func TestFormatter_toComparable(t *testing.T) { //nolint:tparallel // parallel test pattern is intentional.
	t.Parallel()

	formatter := NewFormatter(FormatConfig{})

	tests := []struct {
		value    any
		name     string
		expected float64
	}{
		{3.14, "float64", 3.14},
		{42, "int", 42.0},
		{int32(10), "int32", 10.0},
		{int64(100), "int64", 100.0},
		{"hello", "string", 5.0}, // Length of string.
		{nil, "nil", 0},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := formatter.toComparable(tt.value)
			if result != tt.expected {
				t.Errorf("toComparable(%v) = %f, want %f", tt.value, result, tt.expected)
			}
		})
	}
}
