package complexity

import (
	"fmt"
	"strconv"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/common/reportutil"
	"github.com/Sumatoshi-tech/codefang/pkg/alg/mapx"
)

// Section rendering constants.
const (
	SectionTitle = "COMPLEXITY"

	// ScoreExcellentThreshold is the upper bound of average complexity for an excellent score.
	ScoreExcellentThreshold = 1.0
	ScoreGoodThreshold      = 3.0
	ScoreFairThreshold      = 5.0
	ScoreModerateThreshold  = 7.0
	ScorePoorThreshold      = 10.0

	ScoreExcellent = 1.0
	ScoreGood      = 0.8
	ScoreFair      = 0.6
	ScoreModerate  = 0.4
	ScorePoor      = 0.2
	ScoreCritical  = 0.1

	// DistSimpleMax is the maximum cyclomatic complexity for the "simple" distribution bucket.
	DistSimpleMax    = 5
	DistModerateMax  = 10
	DistComplexMax   = 20
	DistLabelSimple  = "Simple (1-5)"
	DistLabelMod     = "Moderate (6-10)"
	DistLabelComplex = "Complex (11-20)"
	DistLabelVeryC   = "Very Complex (>20)"

	// IssueSeverityFairMin is the minimum cyclomatic complexity for fair severity.
	IssueSeverityFairMin = 6
	IssueSeverityPoorMin = 11
	IssueValuePrefix     = "CC="

	// MetricTotalFunctions is the label for the total functions metric.
	MetricTotalFunctions  = "Total Functions"
	MetricAvgComplexity   = "Avg Complexity"
	MetricMaxComplexity   = "Max Complexity"
	MetricTotalComplexity = "Total Complexity"
	MetricCognitiveTotal  = "Cognitive Total"
	MetricDecisionPoints  = "Decision Points"

	// DefaultStatusMessage is the fallback message when no complexity data is available.
	DefaultStatusMessage = "No complexity data available"

	// KeyAvgComplexity is the report key for average complexity.
	KeyAvgComplexity       = "average_complexity"
	KeyTotalFunctions      = "total_functions"
	KeyMaxComplexity       = "max_complexity"
	KeyTotalComplexity     = "total_complexity"
	KeyCognitiveComplexity = "cognitive_complexity"
	KeyDecisionPoints      = "decision_points"
	KeyMessage             = "message"
	KeyFunctions           = "functions"
	KeyFuncName            = "name"
	KeyFuncCyclomatic      = "cyclomatic_complexity"
	KeyFuncCognitive       = "cognitive_complexity"
	KeyFuncNesting         = "nesting_depth"
)

// ReportSection implements analyze.ReportSection for complexity analysis.
type ReportSection struct {
	analyze.BaseReportSection

	report analyze.Report
}

// NewReportSection creates a ReportSection from a complexity report.
func NewReportSection(report analyze.Report) *ReportSection {
	if report == nil {
		report = analyze.Report{}
	}

	avg := reportutil.GetFloat64(report, KeyAvgComplexity)

	msg := reportutil.GetString(report, KeyMessage)
	if msg == "" {
		msg = DefaultStatusMessage
	}

	return &ReportSection{
		BaseReportSection: analyze.BaseReportSection{
			Title:      SectionTitle,
			Message:    msg,
			ScoreValue: calculateScore(avg),
		},
		report: report,
	}
}

// KeyMetrics returns the 6 key metrics for the complexity section.
func (s *ReportSection) KeyMetrics() []analyze.Metric {
	return []analyze.Metric{
		{Label: MetricTotalFunctions, Value: formatInt(reportutil.GetInt(s.report, KeyTotalFunctions))},
		{Label: MetricAvgComplexity, Value: formatFloat(reportutil.GetFloat64(s.report, KeyAvgComplexity))},
		{Label: MetricMaxComplexity, Value: formatInt(reportutil.GetInt(s.report, KeyMaxComplexity))},
		{Label: MetricTotalComplexity, Value: formatInt(reportutil.GetInt(s.report, KeyTotalComplexity))},
		{Label: MetricCognitiveTotal, Value: formatInt(reportutil.GetInt(s.report, KeyCognitiveComplexity))},
		{Label: MetricDecisionPoints, Value: formatInt(reportutil.GetInt(s.report, KeyDecisionPoints))},
	}
}

// Distribution returns complexity distribution categories.
func (s *ReportSection) Distribution() []analyze.DistributionItem {
	functions := reportutil.GetFunctions(s.report, KeyFunctions)
	if len(functions) == 0 {
		return nil
	}

	counts := categorize(functions)
	total := len(functions)

	return buildDistribution(counts, total)
}

// issueEnvelope carries numeric sort keys alongside the display Issue.
type issueEnvelope struct {
	cognitive  int
	cyclomatic int
	issue      analyze.Issue
	nesting    int
}

// complexityEnvelopeLess orders envelopes by cyclomatic desc, cognitive desc, nesting desc, name asc.
func complexityEnvelopeLess(a, b issueEnvelope) bool {
	if a.cyclomatic != b.cyclomatic {
		return a.cyclomatic > b.cyclomatic
	}

	if a.cognitive != b.cognitive {
		return a.cognitive > b.cognitive
	}

	if a.nesting != b.nesting {
		return a.nesting > b.nesting
	}

	return a.issue.Name < b.issue.Name
}

// TopIssues returns the top N functions sorted by complexity descending.
func (s *ReportSection) TopIssues(n int) []analyze.Issue {
	return s.complexityIssues(n)
}

// AllIssues returns all functions as issues sorted by complexity descending.
func (s *ReportSection) AllIssues() []analyze.Issue {
	return s.complexityIssues(0)
}

// complexityIssues builds issues sorted by complexity descending, limited to limit (0 = all).
func (s *ReportSection) complexityIssues(limit int) []analyze.Issue {
	functions := reportutil.GetFunctions(s.report, KeyFunctions)
	if len(functions) == 0 {
		return nil
	}

	envelopes := make([]issueEnvelope, 0, len(functions))
	for _, fn := range functions {
		name := reportutil.MapString(fn, KeyFuncName)
		cc := reportutil.GetInt(fn, KeyFuncCyclomatic)
		cognitive := reportutil.GetInt(fn, KeyFuncCognitive)
		nesting := reportutil.GetInt(fn, KeyFuncNesting)
		envelopes = append(envelopes, issueEnvelope{
			cyclomatic: cc,
			cognitive:  cognitive,
			nesting:    nesting,
			issue: analyze.Issue{
				Name:     name,
				Value:    fmt.Sprintf("%s%d | Cog=%d | Nest=%d", IssueValuePrefix, cc, cognitive, nesting),
				Severity: severityForComplexity(cc),
			},
		})
	}

	sorted := mapx.SortAndLimit(envelopes, complexityEnvelopeLess, limit)

	result := make([]analyze.Issue, 0, len(sorted))
	for _, item := range sorted {
		result = append(result, item.issue)
	}

	return result
}

// --- Score calculation ---.

func calculateScore(avgComplexity float64) float64 {
	switch {
	case avgComplexity <= ScoreExcellentThreshold:
		return ScoreExcellent
	case avgComplexity <= ScoreGoodThreshold:
		return ScoreGood
	case avgComplexity <= ScoreFairThreshold:
		return ScoreFair
	case avgComplexity <= ScoreModerateThreshold:
		return ScoreModerate
	case avgComplexity <= ScorePoorThreshold:
		return ScorePoor
	default:
		return ScoreCritical
	}
}

// --- Distribution helpers ---.

type distCounts struct {
	simple      int
	moderate    int
	complex     int
	veryComplex int
}

func categorize(functions []map[string]any) distCounts {
	var counts distCounts

	for _, fn := range functions {
		cc := reportutil.GetInt(fn, KeyFuncCyclomatic)
		switch {
		case cc <= DistSimpleMax:
			counts.simple++
		case cc <= DistModerateMax:
			counts.moderate++
		case cc <= DistComplexMax:
			counts.complex++
		default:
			counts.veryComplex++
		}
	}

	return counts
}

func buildDistribution(counts distCounts, total int) []analyze.DistributionItem {
	return []analyze.DistributionItem{
		{Label: DistLabelSimple, Percent: pct(counts.simple, total), Count: counts.simple},
		{Label: DistLabelMod, Percent: pct(counts.moderate, total), Count: counts.moderate},
		{Label: DistLabelComplex, Percent: pct(counts.complex, total), Count: counts.complex},
		{Label: DistLabelVeryC, Percent: pct(counts.veryComplex, total), Count: counts.veryComplex},
	}
}

func pct(count, total int) float64 {
	if total == 0 {
		return 0
	}

	return float64(count) / float64(total)
}

// --- Severity helpers ---.

func severityForComplexity(cc int) string {
	switch {
	case cc >= IssueSeverityPoorMin:
		return analyze.SeverityPoor
	case cc >= IssueSeverityFairMin:
		return analyze.SeverityFair
	default:
		return analyze.SeverityGood
	}
}

// --- Formatting helpers ---.

func formatInt(v int) string {
	return strconv.Itoa(v)
}

func formatFloat(v float64) string {
	return fmt.Sprintf("%.1f", v)
}

// CreateReportSection creates a ReportSection from report data.
func (c *Analyzer) CreateReportSection(report analyze.Report) analyze.ReportSection {
	return NewReportSection(report)
}
