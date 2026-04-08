package composition

import (
	"fmt"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/common/reportutil"
	filehistory "github.com/Sumatoshi-tech/codefang/internal/analyzers/file_history"
)

// Report section display constants.
const (
	sectionTitle     = "COMPOSITION"
	metricTotalFiles = "Total Files"
	metricSource     = "Source Files"
	metricSourcePct  = "Source %"

	statusDefault = "File composition analysis completed"
	statusEmpty   = "No files analyzed"
)

// ReportSection implements analyze.ReportSection for composition analysis.
type ReportSection struct {
	analyze.BaseReportSection

	report analyze.Report
}

// NewReportSection creates a ReportSection from aggregated composition data.
func NewReportSection(report analyze.Report) *ReportSection {
	msg := statusDefault

	total := reportutil.GetInt(report, keyTotalFiles)
	if total == 0 {
		msg = statusEmpty
	}

	return &ReportSection{
		BaseReportSection: analyze.BaseReportSection{
			Title:      sectionTitle,
			Message:    msg,
			ScoreValue: analyze.ScoreInfoOnly,
		},
		report: report,
	}
}

// KeyMetrics returns ordered key metrics for display.
func (s *ReportSection) KeyMetrics() []analyze.Metric {
	total := reportutil.GetInt(s.report, keyTotalFiles)
	breakdown := getBreakdown(s.report)
	sourceCount := breakdown[string(filehistory.CategorySource)]

	return []analyze.Metric{
		{Label: metricTotalFiles, Value: reportutil.FormatInt(total)},
		{Label: metricSource, Value: reportutil.FormatInt(sourceCount)},
		{Label: metricSourcePct, Value: reportutil.FormatPercent(reportutil.Pct(sourceCount, total))},
	}
}

// Distribution returns category breakdown as distribution items.
func (s *ReportSection) Distribution() []analyze.DistributionItem {
	breakdown := getBreakdown(s.report)
	total := reportutil.GetInt(s.report, keyTotalFiles)

	if total == 0 {
		return nil
	}

	items := make([]analyze.DistributionItem, 0, len(filehistory.AllCategories))

	for _, cat := range filehistory.AllCategories {
		count := breakdown[string(cat)]
		if count == 0 {
			continue
		}

		items = append(items, analyze.DistributionItem{
			Label:   string(cat),
			Percent: reportutil.Pct(count, total),
			Count:   count,
		})
	}

	return items
}

// TopIssues returns the top N non-source files as issues.
func (s *ReportSection) TopIssues(n int) []analyze.Issue {
	return s.buildIssues(n)
}

// AllIssues returns all non-source files as issues.
func (s *ReportSection) AllIssues() []analyze.Issue {
	return s.buildIssues(0)
}

// buildIssues creates issues for non-source categories showing file counts.
func (s *ReportSection) buildIssues(limit int) []analyze.Issue {
	breakdown := getBreakdown(s.report)
	total := reportutil.GetInt(s.report, keyTotalFiles)

	if total == 0 {
		return nil
	}

	issues := make([]analyze.Issue, 0, len(filehistory.AllCategories))

	for _, cat := range filehistory.AllCategories {
		if cat == filehistory.CategorySource {
			continue
		}

		count := breakdown[string(cat)]
		if count == 0 {
			continue
		}

		issues = append(issues, analyze.Issue{
			Name:     string(cat),
			Value:    fmt.Sprintf("%d files (%.1f%%)", count, float64(count)/float64(total)*percentMultiplier),
			Severity: severityForCategory(cat),
		})
	}

	if limit > 0 && len(issues) > limit {
		issues = issues[:limit]
	}

	return issues
}

// severityForCategory returns the appropriate severity for a file category.
func severityForCategory(cat filehistory.Category) string {
	switch cat {
	case filehistory.CategoryBinary:
		return analyze.SeverityPoor
	case filehistory.CategorySource,
		filehistory.CategoryVendor,
		filehistory.CategoryGenerated,
		filehistory.CategoryDocumentation,
		filehistory.CategoryConfiguration,
		filehistory.CategoryImage,
		filehistory.CategoryDotFile:
		return analyze.SeverityInfo
	}

	return analyze.SeverityInfo
}

// getBreakdown extracts the breakdown map from a report.
func getBreakdown(report analyze.Report) map[string]int {
	raw, ok := report[keyBreakdown]
	if !ok {
		return nil
	}

	m, isMap := raw.(map[string]int)
	if isMap {
		return m
	}

	return nil
}
