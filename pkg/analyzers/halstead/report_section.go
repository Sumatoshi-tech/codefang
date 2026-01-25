package halstead

import (
	"sort"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/common/reportutil"
)

// Section rendering constants
const (
	SectionTitle = "HALSTEAD"

	// Score thresholds map difficulty to 0-1 score (inverted)
	ScoreExcellentMax = 5.0
	ScoreGoodMax      = 15.0
	ScoreFairMax      = 30.0

	ScoreExcellent = 1.0
	ScoreGood      = 0.8
	ScoreFair      = 0.6
	ScorePoor      = 0.3

	// Metric labels
	MetricTotalFunctions = "Total Functions"
	MetricVocabulary     = "Vocabulary"
	MetricVolume         = "Volume"
	MetricDifficulty     = "Difficulty"
	MetricEffort         = "Effort"
	MetricEstBugs        = "Est. Bugs"

	// Distribution thresholds for volume
	DistLowMax     = 100
	DistMedMax     = 1000
	DistHighMax    = 5000
	DistLabelLow   = "Low (<=100)"
	DistLabelMed   = "Medium (101-1000)"
	DistLabelHigh  = "High (1001-5000)"
	DistLabelVHigh = "Very High (>5000)"

	// Issue severity thresholds for effort
	IssueSeverityFairMin = 10000.0
	IssueSeverityPoorMin = 50000.0
	IssueValuePrefix     = "effort="

	// Report key names
	KeyTotalFunctions = "total_functions"
	KeyVocabulary     = "vocabulary"
	KeyVolume         = "volume"
	KeyDifficulty     = "difficulty"
	KeyEffort         = "effort"
	KeyDeliveredBugs  = "delivered_bugs"
	KeyMessage        = "message"
	KeyFunctions      = "functions"
	KeyFuncName       = "name"
	KeyFuncEffort     = "effort"
	KeyFuncVolume     = "volume"

	// Default status message
	DefaultStatusMessage = "No Halstead data available"
)

// HalsteadReportSection implements analyze.ReportSection for Halstead analysis.
type HalsteadReportSection struct {
	analyze.BaseReportSection
	report analyze.Report
}

// NewHalsteadReportSection creates a ReportSection from a Halstead report.
func NewHalsteadReportSection(report analyze.Report) *HalsteadReportSection {
	if report == nil {
		report = analyze.Report{}
	}

	difficulty := reportutil.GetFloat64(report, KeyDifficulty)
	msg := reportutil.GetString(report, KeyMessage)
	if msg == "" {
		msg = DefaultStatusMessage
	}

	return &HalsteadReportSection{
		BaseReportSection: analyze.BaseReportSection{
			Title:      SectionTitle,
			Message:    msg,
			ScoreValue: calculateScore(difficulty),
		},
		report: report,
	}
}

// KeyMetrics returns the key metrics for the Halstead section.
func (s *HalsteadReportSection) KeyMetrics() []analyze.Metric {
	return []analyze.Metric{
		{Label: MetricTotalFunctions, Value: reportutil.FormatInt(reportutil.GetInt(s.report, KeyTotalFunctions))},
		{Label: MetricVocabulary, Value: reportutil.FormatInt(reportutil.GetInt(s.report, KeyVocabulary))},
		{Label: MetricVolume, Value: reportutil.FormatFloat(reportutil.GetFloat64(s.report, KeyVolume))},
		{Label: MetricDifficulty, Value: reportutil.FormatFloat(reportutil.GetFloat64(s.report, KeyDifficulty))},
		{Label: MetricEffort, Value: reportutil.FormatFloat(reportutil.GetFloat64(s.report, KeyEffort))},
		{Label: MetricEstBugs, Value: reportutil.FormatFloat(reportutil.GetFloat64(s.report, KeyDeliveredBugs))},
	}
}

// Distribution returns volume distribution categories.
func (s *HalsteadReportSection) Distribution() []analyze.DistributionItem {
	functions := reportutil.GetFunctions(s.report, KeyFunctions)
	if len(functions) == 0 {
		return nil
	}

	counts := categorizeVolume(functions)
	total := len(functions)

	return []analyze.DistributionItem{
		{Label: DistLabelLow, Percent: reportutil.Pct(counts.low, total), Count: counts.low},
		{Label: DistLabelMed, Percent: reportutil.Pct(counts.medium, total), Count: counts.medium},
		{Label: DistLabelHigh, Percent: reportutil.Pct(counts.high, total), Count: counts.high},
		{Label: DistLabelVHigh, Percent: reportutil.Pct(counts.veryHigh, total), Count: counts.veryHigh},
	}
}

// TopIssues returns the top N functions with highest effort.
func (s *HalsteadReportSection) TopIssues(n int) []analyze.Issue {
	issues := s.buildSortedIssues()
	if n >= len(issues) {
		return issues
	}
	return issues[:n]
}

// AllIssues returns all functions sorted by effort descending.
func (s *HalsteadReportSection) AllIssues() []analyze.Issue {
	return s.buildSortedIssues()
}

// buildSortedIssues extracts functions sorted by effort descending.
func (s *HalsteadReportSection) buildSortedIssues() []analyze.Issue {
	functions := reportutil.GetFunctions(s.report, KeyFunctions)
	if len(functions) == 0 {
		return nil
	}

	// Sort functions by effort descending before building issues
	sort.Slice(functions, func(i, j int) bool {
		return reportutil.MapFloat64(functions[i], KeyFuncEffort) > reportutil.MapFloat64(functions[j], KeyFuncEffort)
	})

	issues := make([]analyze.Issue, 0, len(functions))
	for _, fn := range functions {
		name := reportutil.MapString(fn, KeyFuncName)
		effort := reportutil.MapFloat64(fn, KeyFuncEffort)
		issues = append(issues, analyze.Issue{
			Name:     name,
			Value:    reportutil.FormatFloat(effort),
			Severity: severityForEffort(effort),
		})
	}

	return issues
}

// --- Score calculation ---

func calculateScore(difficulty float64) float64 {
	switch {
	case difficulty <= ScoreExcellentMax:
		return ScoreExcellent
	case difficulty <= ScoreGoodMax:
		return ScoreGood
	case difficulty <= ScoreFairMax:
		return ScoreFair
	default:
		return ScorePoor
	}
}

// --- Distribution helpers ---

type volumeDistCounts struct {
	low      int
	medium   int
	high     int
	veryHigh int
}

func categorizeVolume(functions []map[string]interface{}) volumeDistCounts {
	var counts volumeDistCounts
	for _, fn := range functions {
		vol := reportutil.MapFloat64(fn, KeyFuncVolume)
		switch {
		case vol <= DistLowMax:
			counts.low++
		case vol <= DistMedMax:
			counts.medium++
		case vol <= DistHighMax:
			counts.high++
		default:
			counts.veryHigh++
		}
	}
	return counts
}

// --- Severity helpers ---

func severityForEffort(effort float64) string {
	switch {
	case effort >= IssueSeverityPoorMin:
		return analyze.SeverityPoor
	case effort >= IssueSeverityFairMin:
		return analyze.SeverityFair
	default:
		return analyze.SeverityGood
	}
}

// CreateReportSection creates a ReportSection from report data.
func (h *HalsteadAnalyzer) CreateReportSection(report analyze.Report) analyze.ReportSection {
	return NewHalsteadReportSection(report)
}
