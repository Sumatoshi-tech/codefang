package couples

import (
	"fmt"
	"sort"
	"strconv"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
)

// Section rendering constants.
const (
	ReportSectionTitle = "COUPLES"

	MetricTotalFiles      = "Total Files"
	MetricTotalDevelopers = "Total Developers"
	MetricTotalCoChanges  = "Total Co-Changes"
	MetricHighlyCoupled   = "Highly Coupled Pairs"
	MetricAvgCoupling     = "Avg Coupling"

	// DistStrongMin is the minimum coupling strength for "Strong" distribution bucket.
	DistStrongMin   = 0.7
	DistModerateMin = 0.4
	DistWeakMin     = 0.1
	DistLabelStrong = "Strong (>70%)"
	DistLabelMod    = "Moderate (40-70%)"
	DistLabelWeak   = "Weak (10-40%)"
	DistLabelNone   = "Minimal (<10%)"

	// IssueSeverityHighMin is the minimum coupling strength for "high" severity issues.
	IssueSeverityHighMin = 0.7
	IssueSeverityMedMin  = 0.4

	DefaultStatusMsg = "No coupling data available"
)

// ReportSection implements analyze.ReportSection for couples analysis.
type ReportSection struct {
	analyze.BaseReportSection

	metrics *ComputedMetrics
}

// NewReportSection creates a ReportSection from a couples report.
func NewReportSection(report analyze.Report) *ReportSection {
	if report == nil {
		report = analyze.Report{}
	}

	metrics, err := ComputeAllMetrics(report)
	if err != nil {
		metrics = &ComputedMetrics{}
	}

	score, msg := computeScore(metrics)

	return &ReportSection{
		BaseReportSection: analyze.BaseReportSection{
			Title:      ReportSectionTitle,
			Message:    msg,
			ScoreValue: score,
		},
		metrics: metrics,
	}
}

func computeScore(m *ComputedMetrics) (score float64, msg string) {
	if m.Aggregate.TotalFiles == 0 {
		return analyze.ScoreInfoOnly, DefaultStatusMsg
	}

	// Score is 1.0 - avg_coupling (lower coupling = better score).
	avg := m.Aggregate.AvgCouplingStrength
	score = 1.0 - avg

	if score < 0 {
		score = 0
	}

	const (
		goodThreshold = 0.7
		fairThreshold = 0.4
	)

	switch {
	case score >= goodThreshold:
		msg = fmt.Sprintf("Good - low coupling across %d files", m.Aggregate.TotalFiles)
	case score >= fairThreshold:
		msg = fmt.Sprintf("Fair - moderate coupling (%d highly coupled pairs)", m.Aggregate.HighlyCoupledPairs)
	default:
		msg = fmt.Sprintf("Poor - high coupling (%d highly coupled pairs need attention)", m.Aggregate.HighlyCoupledPairs)
	}

	return score, msg
}

// KeyMetrics returns the key metrics for the couples section.
func (s *ReportSection) KeyMetrics() []analyze.Metric {
	agg := s.metrics.Aggregate

	return []analyze.Metric{
		{Label: MetricTotalFiles, Value: strconv.Itoa(agg.TotalFiles)},
		{Label: MetricTotalDevelopers, Value: strconv.Itoa(agg.TotalDevelopers)},
		{Label: MetricTotalCoChanges, Value: strconv.FormatInt(agg.TotalCoChanges, 10)},
		{Label: MetricHighlyCoupled, Value: strconv.Itoa(agg.HighlyCoupledPairs)},
		{Label: MetricAvgCoupling, Value: fmt.Sprintf("%.0f%%", agg.AvgCouplingStrength*pctMultiplier)},
	}
}

// Distribution returns coupling strength distribution categories.
func (s *ReportSection) Distribution() []analyze.DistributionItem {
	couples := s.metrics.FileCoupling
	if len(couples) == 0 {
		return nil
	}

	counts := categorizeStrength(couples)
	total := len(couples)

	return []analyze.DistributionItem{
		{Label: DistLabelStrong, Percent: pct(counts.strong, total), Count: counts.strong},
		{Label: DistLabelMod, Percent: pct(counts.moderate, total), Count: counts.moderate},
		{Label: DistLabelWeak, Percent: pct(counts.weak, total), Count: counts.weak},
		{Label: DistLabelNone, Percent: pct(counts.minimal, total), Count: counts.minimal},
	}
}

// TopIssues returns the top N most coupled file pairs.
func (s *ReportSection) TopIssues(n int) []analyze.Issue {
	issues := s.buildSortedIssues()
	if n >= len(issues) {
		return issues
	}

	return issues[:n]
}

// AllIssues returns all coupled file pairs sorted by strength descending.
func (s *ReportSection) AllIssues() []analyze.Issue {
	return s.buildSortedIssues()
}

func (s *ReportSection) buildSortedIssues() []analyze.Issue {
	couples := s.metrics.FileCoupling
	if len(couples) == 0 {
		return nil
	}

	issues := make([]analyze.Issue, 0, len(couples))

	for _, cp := range couples {
		issues = append(issues, analyze.Issue{
			Name:     cp.File1 + " \u2194 " + cp.File2,
			Value:    fmt.Sprintf("%.0f%% (%d\u00d7)", cp.Strength*pctMultiplier, cp.CoChanges),
			Severity: severityForStrength(cp.Strength),
		})
	}

	// Sort by strength descending (highest coupling = most concerning = first).
	sort.Slice(issues, func(i, j int) bool {
		return issues[i].Value > issues[j].Value
	})

	return issues
}

// CreateReportSection creates a ReportSection from report data.
func (c *HistoryAnalyzer) CreateReportSection(report analyze.Report) analyze.ReportSection {
	return NewReportSection(report)
}

// --- Distribution helpers ---.

type strengthDistCounts struct {
	strong   int
	moderate int
	weak     int
	minimal  int
}

func categorizeStrength(couples []FileCouplingData) strengthDistCounts {
	var counts strengthDistCounts

	for _, cp := range couples {
		switch {
		case cp.Strength >= DistStrongMin:
			counts.strong++
		case cp.Strength >= DistModerateMin:
			counts.moderate++
		case cp.Strength >= DistWeakMin:
			counts.weak++
		default:
			counts.minimal++
		}
	}

	return counts
}

func severityForStrength(strength float64) string {
	switch {
	case strength >= IssueSeverityHighMin:
		return analyze.SeverityPoor
	case strength >= IssueSeverityMedMin:
		return analyze.SeverityFair
	default:
		return analyze.SeverityGood
	}
}

func pct(count, total int) float64 {
	if total == 0 {
		return 0
	}

	return float64(count) / float64(total)
}
