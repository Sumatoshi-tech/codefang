package imports

import (
	"slices"
	"sort"
	"strings"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/metrics"
)

// Import complexity thresholds.
const (
	deeplyNestedThreshold = 3
	longPathThreshold     = 5
)

// --- Input Data Types ---

// ReportData is the parsed input data for imports metrics computation.
type ReportData struct {
	Imports []string
	Count   int
}

// ParseReportData extracts ReportData from an analyzer report.
// It handles both in-memory Report keys and binary-decoded JSON keys.
func ParseReportData(report analyze.Report) (*ReportData, error) {
	data := &ReportData{}

	if v, ok := report["imports"].([]string); ok {
		data.Imports = v
	} else if items, ok := report["import_list"]; ok {
		// After binary encode -> JSON decode, "imports" becomes "import_list"
		// and values are structured objects with a "path" field.
		data.Imports = extractImportPaths(items)
	}

	if v, ok := report["count"].(int); ok {
		data.Count = v
	} else if v, ok := report["count"].(float64); ok {
		data.Count = int(v)
	}

	return data, nil
}

// extractImportPaths extracts string paths from a JSON-decoded import_list.
// The list may be []any of map[string]any (each with a "path" key).
func extractImportPaths(items any) []string {
	rawList, ok := items.([]any)
	if !ok {
		return nil
	}

	paths := make([]string, 0, len(rawList))

	for _, item := range rawList {
		if m, isMap := item.(map[string]any); isMap {
			if p, hasPath := m["path"].(string); hasPath {
				paths = append(paths, p)
			}
		}
	}

	return paths
}

// --- Output Data Types ---

// ImportData contains information about a single import.
type ImportData struct {
	Path       string `json:"path"        yaml:"path"`
	Category   string `json:"category"    yaml:"category"`
	IsExternal bool   `json:"is_external" yaml:"is_external"`
}

// ImportCategoryData contains import counts by category.
type ImportCategoryData struct {
	Category string `json:"category" yaml:"category"`
	Count    int    `json:"count"    yaml:"count"`
}

// ImportDependencyData identifies potential dependency issues.
type ImportDependencyData struct {
	Path      string `json:"path"       yaml:"path"`
	RiskLevel string `json:"risk_level" yaml:"risk_level"`
	Reason    string `json:"reason"     yaml:"reason"`
}

// AggregateData contains summary statistics.
type AggregateData struct {
	TotalImports    int     `json:"total_imports"    yaml:"total_imports"`
	ExternalImports int     `json:"external_imports" yaml:"external_imports"`
	InternalImports int     `json:"internal_imports" yaml:"internal_imports"`
	UniquePackages  int     `json:"unique_packages"  yaml:"unique_packages"`
	ExternalRatio   float64 `json:"external_ratio"   yaml:"external_ratio"`
}

// --- Metric Implementations ---

// ImportListMetric computes categorized import list.
type ImportListMetric struct {
	metrics.MetricMeta
}

// NewImportListMetric creates the import list metric.
func NewImportListMetric() *ImportListMetric {
	return &ImportListMetric{
		MetricMeta: metrics.MetricMeta{
			MetricName:        "import_list",
			MetricDisplayName: "Import List",
			MetricDescription: "Categorized list of all imports with external/internal classification.",
			MetricType:        "list",
		},
	}
}

// Compute calculates import list data.
func (m *ImportListMetric) Compute(input *ReportData) []ImportData {
	result := make([]ImportData, 0, len(input.Imports))

	for _, imp := range input.Imports {
		category := categorizeImport(imp)
		isExternal := isExternalImport(imp)

		result = append(result, ImportData{
			Path:       imp,
			Category:   category,
			IsExternal: isExternal,
		})
	}

	// Sort by category then path
	sort.Slice(result, func(i, j int) bool {
		if result[i].Category != result[j].Category {
			return result[i].Category < result[j].Category
		}

		return result[i].Path < result[j].Path
	})

	return result
}

func categorizeImport(imp string) string {
	switch {
	case strings.HasPrefix(imp, ".") || strings.HasPrefix(imp, "/"):
		return "relative"
	case strings.Contains(imp, "/"):
		if isStandardLibrary(imp) {
			return "stdlib"
		}

		return "external"
	default:
		if isStandardLibrary(imp) {
			return "stdlib"
		}

		return "external"
	}
}

func isExternalImport(imp string) bool {
	if strings.HasPrefix(imp, ".") || strings.HasPrefix(imp, "/") {
		return false
	}

	return !isStandardLibrary(imp)
}

func isStandardLibrary(imp string) bool {
	// Common standard library prefixes across languages
	stdlibs := []string{
		// Go
		"fmt", "os", "io", "net", "http", "encoding", "sync", "context", "time",
		"strings", "bytes", "bufio", "path", "filepath", "regexp", "sort", "math",
		// Python
		"sys", "typing", "collections", "itertools", "functools", "json", "re",
		// JavaScript/Node
		"fs", "path", "util", "events", "stream", "crypto", "http", "https",
	}

	base := strings.Split(imp, "/")[0]

	return slices.Contains(stdlibs, base)
}

// ImportCategoryMetric computes import distribution by category.
type ImportCategoryMetric struct {
	metrics.MetricMeta
}

// NewImportCategoryMetric creates the category metric.
func NewImportCategoryMetric() *ImportCategoryMetric {
	return &ImportCategoryMetric{
		MetricMeta: metrics.MetricMeta{
			MetricName:        "import_categories",
			MetricDisplayName: "Import Categories",
			MetricDescription: "Distribution of imports by category (stdlib, external, relative).",
			MetricType:        "aggregate",
		},
	}
}

// Compute calculates import category distribution.
func (m *ImportCategoryMetric) Compute(input *ReportData) []ImportCategoryData {
	categories := make(map[string]int)

	for _, imp := range input.Imports {
		cat := categorizeImport(imp)
		categories[cat]++
	}

	result := make([]ImportCategoryData, 0, len(categories))
	for cat, count := range categories {
		result = append(result, ImportCategoryData{
			Category: cat,
			Count:    count,
		})
	}

	// Sort by count descending
	sort.Slice(result, func(i, j int) bool {
		return result[i].Count > result[j].Count
	})

	return result
}

// ImportDependencyMetric identifies potential dependency issues.
type ImportDependencyMetric struct {
	metrics.MetricMeta
}

// NewImportDependencyMetric creates the dependency metric.
func NewImportDependencyMetric() *ImportDependencyMetric {
	return &ImportDependencyMetric{
		MetricMeta: metrics.MetricMeta{
			MetricName:        "import_dependencies",
			MetricDisplayName: "Import Dependencies",
			MetricDescription: "Identifies potential dependency issues such as deeply nested paths " +
				"or unusual import patterns.",
			MetricType: "risk",
		},
	}
}

// Compute identifies dependency issues.
func (m *ImportDependencyMetric) Compute(input *ReportData) []ImportDependencyData {
	var result []ImportDependencyData

	for _, imp := range input.Imports {
		var riskLevel, reason string

		// Check for deeply nested relative imports.
		if strings.Count(imp, "..") >= deeplyNestedThreshold {
			riskLevel = "MEDIUM"
			reason = "Deeply nested relative import may indicate poor module structure"
		}

		// Check for very long import paths.
		if strings.Count(imp, "/") >= longPathThreshold {
			riskLevel = "LOW"
			reason = "Long import path may indicate overly complex package structure"
		}

		if riskLevel != "" {
			result = append(result, ImportDependencyData{
				Path:      imp,
				RiskLevel: riskLevel,
				Reason:    reason,
			})
		}
	}

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
			MetricName:        "imports_aggregate",
			MetricDisplayName: "Imports Summary",
			MetricDescription: "Aggregate statistics for import analysis including counts and ratios.",
			MetricType:        "aggregate",
		},
	}
}

// Compute calculates aggregate statistics.
func (m *AggregateMetric) Compute(input *ReportData) AggregateData {
	agg := AggregateData{
		TotalImports: len(input.Imports),
	}

	packages := make(map[string]bool)

	for _, imp := range input.Imports {
		if isExternalImport(imp) {
			agg.ExternalImports++
		} else {
			agg.InternalImports++
		}

		// Extract base package
		base := strings.Split(imp, "/")[0]
		packages[base] = true
	}

	agg.UniquePackages = len(packages)

	if agg.TotalImports > 0 {
		agg.ExternalRatio = float64(agg.ExternalImports) / float64(agg.TotalImports)
	}

	return agg
}

// --- Computed Metrics ---

// ComputedMetrics holds all computed metric results for the imports analyzer.
type ComputedMetrics struct {
	ImportList   []ImportData           `json:"import_list"  yaml:"import_list"`
	Categories   []ImportCategoryData   `json:"categories"   yaml:"categories"`
	Dependencies []ImportDependencyData `json:"dependencies" yaml:"dependencies"`
	Aggregate    AggregateData          `json:"aggregate"    yaml:"aggregate"`
}

// Analyzer name constant for MetricsOutput interface.
const analyzerNameImports = "imports"

// AnalyzerName returns the name of the analyzer.
func (m *ComputedMetrics) AnalyzerName() string {
	return analyzerNameImports
}

// ToJSON returns the metrics as a JSON-serializable object.
func (m *ComputedMetrics) ToJSON() any {
	return m
}

// ToYAML returns the metrics as a YAML-serializable object.
func (m *ComputedMetrics) ToYAML() any {
	return m
}

// ComputeAllMetrics runs all imports metrics and returns the results.
func ComputeAllMetrics(report analyze.Report) (*ComputedMetrics, error) {
	input, err := ParseReportData(report)
	if err != nil {
		return nil, err
	}

	listMetric := NewImportListMetric()
	importList := listMetric.Compute(input)

	catMetric := NewImportCategoryMetric()
	categories := catMetric.Compute(input)

	depMetric := NewImportDependencyMetric()
	dependencies := depMetric.Compute(input)

	aggMetric := NewAggregateMetric()
	aggregate := aggMetric.Compute(input)

	return &ComputedMetrics{
		ImportList:   importList,
		Categories:   categories,
		Dependencies: dependencies,
		Aggregate:    aggregate,
	}, nil
}
