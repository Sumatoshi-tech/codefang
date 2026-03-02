package cohesion

import (
	"sort"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/common/reportutil"
)

// Section rendering constants.
const (
	SectionTitle = "COHESION"

	// MetricTotalFunctions and related constants define metric labels.
	MetricTotalFunctions   = "Total Functions"
	MetricLCOM             = "LCOM Score"
	MetricCohesionScore    = "Cohesion Score"
	MetricFunctionCohesion = "Avg Cohesion"

	// DistExcellentMin and related constants define distribution thresholds for function cohesion.
	DistExcellentMin   = 0.6
	DistGoodMin        = 0.4
	DistFairMin        = 0.3
	DistLabelExcellent = "Excellent (>0.6)"
	DistLabelGood      = "Good (0.4-0.6)"
	DistLabelFair      = "Fair (0.3-0.4)"
	DistLabelPoor      = "Poor (<0.3)"

	// IssueSeverityFairMax and related constants define issue severity thresholds.
	IssueSeverityFairMax = 0.4
	IssueSeverityPoorMax = 0.3
	IssueValuePrefix     = "cohesion="

	// KeyTotalFunctions and related constants define report key names.
	KeyTotalFunctions   = "total_functions"
	KeyLCOM             = "lcom"
	KeyCohesionScore    = "cohesion_score"
	KeyFunctionCohesion = "function_cohesion"
	KeyMessage          = "message"
	KeyFunctions        = "functions"
	KeyFuncName         = "name"
	KeyFuncCohesion     = "cohesion"

	// DefaultStatusMessage is the default status message.
	DefaultStatusMessage = "No cohesion data available"
)

// ReportSection implements analyze.ReportSection for cohesion analysis.
type ReportSection struct {
	analyze.BaseReportSection

	report analyze.Report
}

// NewReportSection creates a ReportSection from a cohesion report.
func NewReportSection(report analyze.Report) *ReportSection {
	if report == nil {
		report = analyze.Report{}
	}

	score := reportutil.GetFloat64(report, KeyCohesionScore)

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

// KeyMetrics returns the key metrics for the cohesion section.
func (s *ReportSection) KeyMetrics() []analyze.Metric {
	return []analyze.Metric{
		{Label: MetricTotalFunctions, Value: reportutil.FormatInt(reportutil.GetInt(s.report, KeyTotalFunctions))},
		{Label: MetricLCOM, Value: reportutil.FormatFloat(reportutil.GetFloat64(s.report, KeyLCOM))},
		{Label: MetricCohesionScore, Value: reportutil.FormatFloat(reportutil.GetFloat64(s.report, KeyCohesionScore))},
		{Label: MetricFunctionCohesion, Value: reportutil.FormatFloat(reportutil.GetFloat64(s.report, KeyFunctionCohesion))},
	}
}

// Distribution returns cohesion distribution categories.
func (s *ReportSection) Distribution() []analyze.DistributionItem {
	functions := reportutil.GetFunctions(s.report, KeyFunctions)
	if len(functions) == 0 {
		return nil
	}

	counts := categorizeCohesion(functions)
	total := len(functions)

	return []analyze.DistributionItem{
		{Label: DistLabelExcellent, Percent: reportutil.Pct(counts.excellent, total), Count: counts.excellent},
		{Label: DistLabelGood, Percent: reportutil.Pct(counts.good, total), Count: counts.good},
		{Label: DistLabelFair, Percent: reportutil.Pct(counts.fair, total), Count: counts.fair},
		{Label: DistLabelPoor, Percent: reportutil.Pct(counts.poor, total), Count: counts.poor},
	}
}

// TopIssues returns the top N functions with lowest cohesion.
func (s *ReportSection) TopIssues(n int) []analyze.Issue {
	issues := s.buildSortedIssues()
	if n >= len(issues) {
		return issues
	}

	return issues[:n]
}

// AllIssues returns all functions sorted by cohesion ascending (worst first).
func (s *ReportSection) AllIssues() []analyze.Issue {
	return s.buildSortedIssues()
}

// buildSortedIssues extracts functions sorted by cohesion ascending.
func (s *ReportSection) buildSortedIssues() []analyze.Issue {
	functions := reportutil.GetFunctions(s.report, KeyFunctions)
	if len(functions) == 0 {
		return nil
	}

	issues := make([]analyze.Issue, 0, len(functions))
	for _, fn := range functions {
		name := reportutil.MapString(fn, KeyFuncName)
		coh := reportutil.GetFloat64(fn, KeyFuncCohesion)
		issues = append(issues, analyze.Issue{
			Name:     name,
			Value:    reportutil.FormatFloat(coh),
			Severity: severityForCohesion(coh),
		})
	}

	// Sort ascending (lowest cohesion = worst = first).
	sort.Slice(issues, func(i, j int) bool {
		return issues[i].Value < issues[j].Value
	})

	return issues
}

// --- Distribution helpers ---.

type cohesionDistCounts struct {
	excellent int
	good      int
	fair      int
	poor      int
}

func categorizeCohesion(functions []map[string]any) cohesionDistCounts {
	var counts cohesionDistCounts

	for _, fn := range functions {
		coh := reportutil.GetFloat64(fn, KeyFuncCohesion)
		switch {
		case coh >= DistExcellentMin:
			counts.excellent++
		case coh >= DistGoodMin:
			counts.good++
		case coh >= DistFairMin:
			counts.fair++
		default:
			counts.poor++
		}
	}

	return counts
}

// --- Severity helpers ---.

func severityForCohesion(coh float64) string {
	switch {
	case coh < IssueSeverityPoorMax:
		return analyze.SeverityPoor
	case coh < IssueSeverityFairMax:
		return analyze.SeverityFair
	default:
		return analyze.SeverityGood
	}
}

// CreateReportSection creates a ReportSection from report data.
func (c *Analyzer) CreateReportSection(report analyze.Report) analyze.ReportSection {
	return NewReportSection(report)
}
