package imports

import (
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/common/reportutil"
	"github.com/Sumatoshi-tech/codefang/pkg/alg/mapx"
)

// Section rendering constants.
const (
	SectionTitle = "IMPORTS"

	// MetricUniqueImports is the label for the unique imports metric.
	MetricUniqueImports = "Unique Imports"
	MetricTotalFiles    = "Total Files"

	// KeyImports is the report key for the list of imports.
	KeyImports      = "imports"
	KeyCount        = "count"
	KeyTotalFiles   = "total_files"
	KeyImportCounts = "import_counts"

	// DefaultStatusMessage is the fallback message when no import data is available.
	DefaultStatusMessage = "No import data available"
	StatusMessagePrefix  = "Found "
	StatusMessageSuffix  = " unique imports"
)

// ReportSection implements analyze.ReportSection for import analysis.
// This is an info-only section (no score).
type ReportSection struct {
	analyze.BaseReportSection

	report analyze.Report
}

// NewReportSection creates a ReportSection from an imports report.
func NewReportSection(report analyze.Report) *ReportSection {
	if report == nil {
		report = analyze.Report{}
	}

	count := reportutil.GetInt(report, KeyCount)
	msg := buildStatusMessage(count)

	return &ReportSection{
		BaseReportSection: analyze.BaseReportSection{
			Title:      SectionTitle,
			Message:    msg,
			ScoreValue: analyze.ScoreInfoOnly,
		},
		report: report,
	}
}

// KeyMetrics returns the key metrics for the imports section.
func (s *ReportSection) KeyMetrics() []analyze.Metric {
	return []analyze.Metric{
		{Label: MetricUniqueImports, Value: reportutil.FormatInt(reportutil.GetInt(s.report, KeyCount))},
		{Label: MetricTotalFiles, Value: reportutil.FormatInt(reportutil.GetInt(s.report, KeyTotalFiles))},
	}
}

// Distribution returns nil for imports (no distribution).
func (s *ReportSection) Distribution() []analyze.DistributionItem {
	return nil
}

// importEntry holds an import name with its count for sorting.
type importEntry struct {
	name  string
	count int
}

// importEntryLess orders entries by count descending (most used = first).
func importEntryLess(a, b importEntry) bool { return a.count > b.count }

// importNameLess orders string imports alphabetically ascending.
func importNameLess(a, b string) bool { return a < b }

// TopIssues returns the top N most used imports as info items.
func (s *ReportSection) TopIssues(n int) []analyze.Issue {
	return s.importIssues(n)
}

// AllIssues returns all imports as info items.
func (s *ReportSection) AllIssues() []analyze.Issue {
	return s.importIssues(0)
}

// importIssues builds import issues sorted by frequency (or name), limited to limit (0 = all).
func (s *ReportSection) importIssues(limit int) []analyze.Issue {
	counts := reportutil.GetStringIntMap(s.report, KeyImportCounts)
	if len(counts) > 0 {
		return buildIssuesFromCounts(counts, limit)
	}

	// Fallback: use simple imports list.
	imports := reportutil.GetStringSlice(s.report, KeyImports)
	if len(imports) == 0 {
		return nil
	}

	return buildIssuesFromList(imports, limit)
}

// buildIssuesFromCounts creates sorted issues from import_counts map.
func buildIssuesFromCounts(counts map[string]int, limit int) []analyze.Issue {
	entries := make([]importEntry, 0, len(counts))
	for name, count := range counts {
		entries = append(entries, importEntry{name: name, count: count})
	}

	// Sort by count descending (numeric, not string).
	sorted := mapx.SortAndLimit(entries, importEntryLess, limit)

	issues := make([]analyze.Issue, 0, len(sorted))
	for _, e := range sorted {
		issues = append(issues, analyze.Issue{
			Name:     e.name,
			Value:    reportutil.FormatInt(e.count),
			Severity: analyze.SeverityInfo,
		})
	}

	return issues
}

// buildIssuesFromList creates issues from a simple string slice sorted alphabetically.
func buildIssuesFromList(imports []string, limit int) []analyze.Issue {
	sorted := mapx.SortAndLimit(imports, importNameLess, limit)

	issues := make([]analyze.Issue, 0, len(sorted))
	for _, imp := range sorted {
		issues = append(issues, analyze.Issue{
			Name:     imp,
			Value:    "1",
			Severity: analyze.SeverityInfo,
		})
	}

	return issues
}

// buildStatusMessage creates a status message from the import count.
func buildStatusMessage(count int) string {
	if count == 0 {
		return DefaultStatusMessage
	}

	return StatusMessagePrefix + reportutil.FormatInt(count) + StatusMessageSuffix
}

// CreateReportSection creates a ReportSection from report data.
func (a *Analyzer) CreateReportSection(report analyze.Report) analyze.ReportSection {
	return NewReportSection(report)
}
