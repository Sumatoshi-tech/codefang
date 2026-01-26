package comments

import (
	"sort"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/common/reportutil"
)

// Section rendering constants.
const (
	SectionTitle = "COMMENTS"

	// MetricTotalComments score is overall_score directly (already 0-1).

	// MetricTotalComments and related constants define metric labels.
	MetricTotalComments  = "Total Comments"
	MetricGoodComments   = "Good Comments"
	MetricBadComments    = "Bad Comments"
	MetricDocCoverage    = "Doc Coverage"
	MetricGoodRatio      = "Good Ratio"
	MetricTotalFunctions = "Total Functions"

	// DistLabelDocumented is the distribution label for documented functions.
	DistLabelDocumented   = "Documented"
	DistLabelUndocumented = "Undocumented"

	// IssueAssessmentBad is the assessment value indicating a function has no comment.
	IssueAssessmentBad = "❌ No Comment"
	IssueValueNoDoc    = "undocumented"

	// KeyOverallScore is the report key for the overall comments score.
	KeyOverallScore   = "overall_score"
	KeyTotalComments  = "total_comments"
	KeyGoodComments   = "good_comments"
	KeyBadComments    = "bad_comments"
	KeyDocCoverage    = "documentation_coverage"
	KeyGoodRatio      = "good_comments_ratio"
	KeyTotalFunctions = "total_functions"
	KeyDocFunctions   = "documented_functions"
	KeyMessage        = "message"
	KeyFunctions      = "functions"
	KeyFuncName       = "function"
	KeyFuncAssessment = "assessment"

	// DefaultStatusMessage is the fallback message when no comment data is available.
	DefaultStatusMessage = "No comment data available"
)

// ReportSection implements analyze.ReportSection for comments analysis.
type ReportSection struct {
	analyze.BaseReportSection

	report analyze.Report
}

// NewReportSection creates a ReportSection from a comments report.
func NewReportSection(report analyze.Report) *ReportSection {
	if report == nil {
		report = analyze.Report{}
	}

	score := reportutil.GetFloat64(report, KeyOverallScore)

	msg := reportutil.GetString(report, KeyMessage)
	if msg == "" {
		msg = DefaultStatusMessage
	}

	return &ReportSection{
		BaseReportSection: analyze.BaseReportSection{
			Title:      SectionTitle,
			Message:    msg,
			ScoreValue: score,
		},
		report: report,
	}
}

// KeyMetrics returns the key metrics for the comments section.
func (s *ReportSection) KeyMetrics() []analyze.Metric {
	return []analyze.Metric{
		{Label: MetricTotalComments, Value: reportutil.FormatInt(reportutil.GetInt(s.report, KeyTotalComments))},
		{Label: MetricGoodComments, Value: reportutil.FormatInt(reportutil.GetInt(s.report, KeyGoodComments))},
		{Label: MetricBadComments, Value: reportutil.FormatInt(reportutil.GetInt(s.report, KeyBadComments))},
		{Label: MetricDocCoverage, Value: reportutil.FormatPercent(reportutil.GetFloat64(s.report, KeyDocCoverage))},
		{Label: MetricGoodRatio, Value: reportutil.FormatPercent(reportutil.GetFloat64(s.report, KeyGoodRatio))},
		{Label: MetricTotalFunctions, Value: reportutil.FormatInt(reportutil.GetInt(s.report, KeyTotalFunctions))},
	}
}

// Distribution returns documented vs undocumented function distribution.
func (s *ReportSection) Distribution() []analyze.DistributionItem {
	total := reportutil.GetInt(s.report, KeyTotalFunctions)
	if total == 0 {
		return nil
	}

	documented := reportutil.GetInt(s.report, KeyDocFunctions)
	undocumented := total - documented

	return []analyze.DistributionItem{
		{Label: DistLabelDocumented, Percent: reportutil.Pct(documented, total), Count: documented},
		{Label: DistLabelUndocumented, Percent: reportutil.Pct(undocumented, total), Count: undocumented},
	}
}

// TopIssues returns the top N undocumented functions.
func (s *ReportSection) TopIssues(n int) []analyze.Issue {
	issues := s.buildIssues()
	if n >= len(issues) {
		return issues
	}

	return issues[:n]
}

// AllIssues returns all undocumented functions.
func (s *ReportSection) AllIssues() []analyze.Issue {
	return s.buildIssues()
}

// buildIssues extracts undocumented functions from the report.
func (s *ReportSection) buildIssues() []analyze.Issue {
	functions := reportutil.GetFunctions(s.report, KeyFunctions)
	if len(functions) == 0 {
		return nil
	}

	var issues []analyze.Issue

	for _, fn := range functions {
		assessment := reportutil.MapString(fn, KeyFuncAssessment)
		if assessment != IssueAssessmentBad {
			continue
		}

		name := reportutil.MapString(fn, KeyFuncName)
		issues = append(issues, analyze.Issue{
			Name:     name,
			Value:    IssueValueNoDoc,
			Severity: analyze.SeverityPoor,
		})
	}

	sort.Slice(issues, func(i, j int) bool {
		return issues[i].Name < issues[j].Name
	})

	return issues
}

// CreateReportSection creates a ReportSection from report data.
func (c *Analyzer) CreateReportSection(report analyze.Report) analyze.ReportSection {
	return NewReportSection(report)
}
